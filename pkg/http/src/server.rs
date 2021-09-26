use std::collections::HashMap;
use std::convert::TryFrom;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use common::async_std::channel;
use common::async_std::net::{TcpListener, TcpStream};
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::errors::*;
use common::futures::stream::StreamExt;
use common::io::*;
use common::CancellationToken;
use net::ip::IPAddress;

use crate::message::*;
use crate::message_body::{decode_request_body_v1, encode_response_body_v1};
use crate::message_syntax::*;
use crate::method::*;
use crate::reader::*;
use crate::request::*;
use crate::response::*;
use crate::spec::*;
use crate::status_code::*;
use crate::v2;

// TODO: See https://tools.ietf.org/html/rfc7230#section-3.5 for
// robustness tips and accepting empty lines before a request-line.

// TODO: See https://tools.ietf.org/html/rfc7230#section-3.3.3 with
// special HEAD/status code behavior

#[derive(Clone)]
pub struct ServerOptions {
    /// If true, we will only accept HTTPv2 connections. Setting this to true
    /// will improve the performance of V2 connections as we will internally
    /// bypass buffering done with V1.
    pub force_http2: bool,

    pub connection_options_v2: v2::ConnectionOptions,

    /// TODO: Provide a reasonable default and implement me.
    pub max_num_connections: usize,

    /// For v2 connections, min(graceful_shutdown_timeout,
    /// connection_options_v2.server_graceful_shutdown_timeout) will effectively
    /// be used
    pub graceful_shutdown_timeout: Duration,
}

impl Default for ServerOptions {
    fn default() -> Self {
        let connection_options_v2 = v2::ConnectionOptions::default();

        Self {
            force_http2: false,
            connection_options_v2: connection_options_v2.clone(),
            max_num_connections: 1000,
            graceful_shutdown_timeout: connection_options_v2
                .server_graceful_shutdown_timeout
                .clone(),
        }
    }
}

#[async_trait]
pub trait RequestHandler: 'static + Send + Sync {
    /// Processes an HTTP request returning a response eventually.
    ///
    /// While the full request is available in the first argument, the following
    /// headers are handled automatically in the server:
    /// - Content-Length
    /// - Transfer-Encoding
    /// - Connection
    /// - Keep-Alive
    /// - TE
    /// - Host
    async fn handle_request(&self, request: Request) -> Response;
}

#[async_trait]
impl<T: RequestHandler> RequestHandler for Arc<T> {
    async fn handle_request(&self, request: Request) -> Response {
        self.as_ref().handle_request(request).await
    }
}

/// Wraps a simple static function as a server request handler.
/// See RequestHandler::handle_request for more information.
pub fn HttpFn<
    F: Future<Output = Response> + Send + 'static,
    H: (Fn(Request) -> F) + Send + Sync + 'static,
>(
    handler_fn: H,
) -> RequestHandlerFnCaller {
    RequestHandlerFnCaller {
        value: Box::new(move |req| Box::pin(handler_fn(req))),
    }
}

/// Internal: Used by HttpFn.
pub struct RequestHandlerFnCaller {
    value: Box<dyn (Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>>) + Send + Sync>,
}

#[async_trait]
impl RequestHandler for RequestHandlerFnCaller {
    async fn handle_request(&self, request: Request) -> Response {
        (self.value)(request).await
    }
}

struct RequestContext {
    pub secure: bool,

    pub peer_addr: IPAddress,
    // TODO: In the future, it will also be useful to have HTTP2 specific information.
}

// TODO: Start shutdown when dropped?

/// Receives HTTP requests and parses them.
/// Passes the request to a handler which can produce a response.
pub struct Server {
    shared: Arc<ServerShared>,

    shutdown_token: Option<Box<dyn CancellationToken>>,

    shutdown_timer: Option<task::JoinHandle<()>>,
}

struct ServerShared {
    handler: Box<dyn RequestHandler>,

    options: ServerOptions,

    connection_pool: Mutex<ServerConnectionPool>,

    // TODO: Make the channels broadcast to all listeners in the case that we call run() multiple
    // times.
    /// An event is sent on this channel whenever we remove a connection from
    /// the connection pool and that causes the pool to become empty.
    ///
    /// This is used during server shutdown to know when we are done shutting
    /// down.
    connection_pool_empty_channel: (channel::Sender<()>, channel::Receiver<()>),

    shutting_down: AtomicBool,
}

// TODO: We could possibly improve performance if instead of maintaining a
// connection map, we simply maintain a AtomicUsize with the number of
// connections and have a way to copy and propagate the cancellation token to
// individual connection tasks.
struct ServerConnectionPool {
    connections: HashMap<usize, ServerConnection>,
    last_id: usize,
}

struct ServerConnection {
    /// Handle to the main task that is serving this connection
    task_handle: task::JoinHandle<()>,

    mode: ServerConnectionMode,
}

enum ServerConnectionMode {
    /// In this mode, we are still waiting for the client to send a message head
    /// indicating which version it wants to use.
    Unknown,

    V1,

    V2(v2::Connection),
}

enum ServerConnectionV2Input {
    Raw,
    Upgrade(Request),
    SkipPrefaceHead,
}

impl Drop for Server {
    fn drop(&mut self) {
        if !self.shutdown_timer.is_some() {
            let shared = self.shared.clone();
            // TODO: If all connections die earlier than this timeout, then we should
            // support cleaning up this timeout.
            task::spawn(Self::run_shutdown_timer(shared));
        }
    }
}

impl Server {
    pub fn new<H: RequestHandler>(handler: H, options: ServerOptions) -> Self {
        Self {
            shared: Arc::new(ServerShared {
                handler: Box::new(handler),
                options,
                connection_pool: Mutex::new(ServerConnectionPool {
                    connections: HashMap::new(),
                    last_id: 0,
                }),
                connection_pool_empty_channel: channel::bounded(1),
                shutting_down: AtomicBool::new(false),
            }),
            shutdown_token: None,
            shutdown_timer: None,
        }
    }

    pub fn set_shutdown_token(&mut self, token: Box<dyn CancellationToken>) {
        self.shutdown_token = Some(token);
    }

    /// Start the shutdown of the server.
    /// The shutdown is finished when run() has returned.
    ///
    /// Shutdown internal behaviors:
    /// - We will immediately stop accepting new connections.
    /// - If graceful = true,
    ///   - For all ongoing HTTPv2 connections, we will call
    ///     v2::Connection::shutdown(true)
    ///   - For all ongoing HTTPv1|Unknown connections, we will close the
    ///     connection after when the next response is fully sent out.
    ///     - This means that if the client is using pipelining, that all future
    ///       requests already enqueued on the connection might have already
    ///       been partially processed.
    ///   - We will start a timeout to start an abrupt shutdown
    /// - If abruptly shutting down, (graceful = false),
    ///   - We will call v2::Connection::shutdown(false) for all HTTPv2
    ///     connections.
    ///   - We will cancel all Unknown version or V1 connection tasks
    ///     immediately.
    /// - In all cases, shutdown is over once all connection tasks have exited.
    ///
    /// TODO: Currently this is always called with graceful.
    async fn shutdown(&mut self, graceful: bool) {
        Self::shutdown_impl(&self.shared, graceful).await;

        if graceful && !self.shutdown_timer.is_some() {
            let shared = self.shared.clone();
            self.shutdown_timer = Some(task::spawn(Self::run_shutdown_timer(shared)));
        }
    }

    /// TODO: Use a weak pointer?
    async fn run_shutdown_timer(shared: Arc<ServerShared>) {
        common::wait_for(shared.options.graceful_shutdown_timeout).await;
        Self::shutdown_impl(&shared, false).await;
    }

    async fn shutdown_impl(shared: &ServerShared, graceful: bool) {
        shared.shutting_down.store(true, Ordering::Relaxed);

        // TODO: Spawn in this in a separate task so that it can't be interrupted.

        let mut connection_pool = shared.connection_pool.lock().await;

        let mut cancel_ids = vec![];

        for (connection_id, connection) in &mut connection_pool.connections {
            let connection_id = *connection_id;
            match &connection.mode {
                ServerConnectionMode::Unknown | ServerConnectionMode::V1 => {
                    if !graceful {
                        cancel_ids.push(connection_id);
                    }
                }
                ServerConnectionMode::V2(conn_v2) => {
                    conn_v2.shutdown(graceful).await;
                }
            }
        }

        for cancel_id in cancel_ids {
            let conn = connection_pool.connections.remove(&cancel_id).unwrap();
            conn.task_handle.cancel().await;
        }

        if connection_pool.connections.is_empty() {
            let _ = shared.connection_pool_empty_channel.0.try_send(());
        }
    }

    // TODO: Ideally we'd support using some alternative connection (e.g. a
    // TlsServer)

    /// Starts listening on the given port and processes new connections until
    /// the server is shut down.
    pub async fn run(mut self, port: u16) -> Result<()> {
        enum Event {
            NextStream(Option<Result<TcpStream>>),
            Shutdown,
        }

        // Bind all all interfaces.
        let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).await?;

        let mut incoming = listener.incoming();
        loop {
            let next_stream = common::future::map(
                incoming.next(),
                |v: Option<std::result::Result<TcpStream, std::io::Error>>| {
                    Event::NextStream(v.map(|r| r.map_err(|e| Error::from(e))))
                },
            );

            let event = {
                if let Some(shutdown_token) = &self.shutdown_token {
                    let shutdown_event =
                        common::future::map(shutdown_token.wait(), |_| Event::Shutdown);

                    common::future::race(next_stream, shutdown_event).await
                } else {
                    next_stream.await
                }
            };

            match event {
                Event::NextStream(Some(stream)) => {
                    let s = stream?;

                    let mut connection_pool = self.shared.connection_pool.lock().await;

                    // TODO: Support over usize # of connections by wrapping and checking if the id
                    // is already in the hashmap.
                    let connection_id = connection_pool.last_id + 1;
                    connection_pool.last_id = connection_id;

                    let task_handle =
                        task::spawn(Self::handle_stream(self.shared.clone(), connection_id, s));

                    connection_pool.connections.insert(
                        connection_id,
                        ServerConnection {
                            task_handle,
                            mode: ServerConnectionMode::Unknown,
                        },
                    );
                }
                Event::NextStream(None) => {
                    return Err(err_msg("Listener ended early"));
                }
                Event::Shutdown => {
                    self.shutdown(true).await;
                    break;
                }
            }
        }

        // TODO: Verify that when we stop accepting connections, any active connections
        // stay active.
        drop(incoming);
        drop(listener);

        // Wait for all connections to die.
        loop {
            {
                let connection_pool = self.shared.connection_pool.lock().await;
                if connection_pool.connections.is_empty() {
                    break;
                }
            }

            let _ = self.shared.connection_pool_empty_channel.1.recv().await;
        }

        Ok(())
    }

    /*
        TODO: We want to have a way of introspecting a request stream to see things like the client's IP.
    */
    // TODO: Should be refactored to
    async fn handle_stream(shared: Arc<ServerShared>, connection_id: usize, stream: TcpStream) {
        match Self::handle_stream_impl(&shared, connection_id, stream).await {
            Ok(v) => {}
            // TODO: If we see a ProtocolErrorV1, form an HTTP 1.1 response.
            // (but only if generated locally )
            // A ProtocolErrorV2 should probably also be a
            Err(e) => println!("Connection thread failed: {}", e),
        };

        // Now that the connection is finished, remove it from the global list.
        {
            let mut connection_pool = shared.connection_pool.lock().await;
            connection_pool.connections.remove(&connection_id);
            if connection_pool.connections.is_empty() {
                let _ = shared.connection_pool_empty_channel.0.try_send(());
            }
        }
    }

    // TODO: Verify that the HTTP2 error handling works ok.

    // TODO: Errors in here should close the connection.
    async fn handle_stream_impl(
        shared: &Arc<ServerShared>,
        connection_id: usize,
        stream: TcpStream,
    ) -> Result<()> {
        if shared.options.force_http2 {
            let reader = Box::new(stream.clone());
            let writer = Box::new(stream);
            return Self::handle_stream_v2(
                shared,
                connection_id,
                reader,
                writer,
                ServerConnectionV2Input::Raw,
            )
            .await;
        }

        let mut write_stream = stream.clone();
        let mut read_stream = PatternReader::new(Box::new(stream), MESSAGE_HEAD_BUFFER_OPTIONS);

        loop {
            let head = match read_http_message(&mut read_stream).await? {
                HttpStreamEvent::MessageHead(h) => h,
                HttpStreamEvent::HeadersTooLarge => {
                    return Err(ProtocolErrorV1 {
                        code: REQUEST_HEADER_FIELDS_TOO_LARGE,
                        message: "",
                    }
                    .into());
                }
                HttpStreamEvent::EndOfStream | HttpStreamEvent::Incomplete(_) => {
                    return Ok(());
                }
            };

            let msg = match parse_http_message_head(head) {
                Ok((msg, rest)) => {
                    assert_eq!(rest.len(), 0);
                    msg
                }
                Err(e) => {
                    println!("Failed to parse message\n{}", e);
                    write_stream
                        .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            };

            let start_line = msg.start_line;
            let headers = msg.headers;

            // Verify that we got a Request style message
            let request_line = match start_line {
                StartLine::Request(r) => r,
                StartLine::Response(r) => {
                    println!("Unexpected response: {:?}", r);
                    write_stream
                        .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            };

            // TODO: If we previously negotiated HTTP2, complain if we didn't actually end
            // up using it.

            // TODO: In HTTP2, does the client need to send a headers frame?
            // TODO: "Upon receiving the 101 response, the client MUST send a connection
            // preface (Section 3.5), which includes a SETTINGS frame"

            // Verify supported HTTP version
            match request_line.version {
                HTTP_V0_9 => {}
                HTTP_V1_0 => {}
                HTTP_V1_1 => {}
                HTTP_V2_0 => {
                    // In this case, we received the first two lines of the HTTP 2 connection
                    // preface which should always be "PRI * HTTP/2.0\r\n\r\n"
                    if request_line.method.as_ref() != "PRI"
                        || request_line.target != RequestTarget::AsteriskForm
                        || !headers.raw_headers.is_empty()
                    {
                        return Err(ProtocolErrorV1 {
                            code: BAD_REQUEST,
                            message: "Incorrect start line for HTTP 2.0",
                        }
                        .into());
                    }

                    return Self::handle_stream_v2(
                        shared,
                        connection_id,
                        Box::new(read_stream),
                        Box::new(write_stream),
                        ServerConnectionV2Input::SkipPrefaceHead,
                    )
                    .await;
                }
                _ => {
                    println!("Unsupported http version: {:?}", request_line.version);
                    write_stream
                        .write_all(b"HTTP/1.1 505 HTTP Version Not Supported\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            };

            // Validate method
            let method = match Method::try_from(request_line.method.data.as_ref()) {
                Ok(m) => m,
                Err(_) => {
                    // TODO: Switch to using a ProtocolErrorV1
                    println!("Unsupported http method: {:?}", request_line.method);
                    write_stream
                        .write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            };

            let mut request_head = RequestHead {
                method,
                uri: request_line.target.into_uri(),
                version: request_line.version,
                headers,
            };

            // TODO:
            // A server MUST respond with a 400 (Bad Request) status code to any
            // HTTP/1.1 request message that lacks a Host header field and to any
            // request message that contains more than one Host header field or a
            // Host header field with an invalid field-value.
            // ^ Do this. Move it into the uri. (unless the URI already has one)
            let host = match crate::headers::host::parse_host_header(&request_head.headers) {
                Ok(v) => v,
                Err(e) => {
                    println!("{}", e);
                    write_stream
                        .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            };

            if let Some(host) = host {
                // According to RFC 7230 Section 5.4, if the request target received if in
                // absolute-form, the Host header should be ignored.
                if !request_head.uri.authority.is_some() {
                    request_head.uri.authority = Some(host);
                }
            } else {
                if request_head.version == HTTP_V1_1 {
                    write_stream
                        .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            }

            // TODO: Convert the error into a response.
            let mut persist_connection = crate::headers::connection::can_connection_persist(
                &request_head.version,
                &request_head.headers,
            )?;

            let (body, mut reader_waiter) = match decode_request_body_v1(&request_head, read_stream)
            {
                Ok(pair) => pair,
                Err(e) => {
                    println!("{}", e);
                    write_stream
                        .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            };

            if reader_waiter.is_none() {
                persist_connection = false;
            }

            let req = Request {
                head: request_head,
                body,
            };

            let upgrade_protocols =
                crate::headers::upgrade_syntax::parse_upgrade(&req.head.headers)?;

            let mut has_h2c_upgrade = false;
            for protocol in &upgrade_protocols {
                if protocol.name.as_ref() == "h2c" && protocol.version.is_none() {
                    has_h2c_upgrade = true;
                    break;
                }
            }

            if has_h2c_upgrade {
                // TODO:

                let reader_waiter = match reader_waiter.take() {
                    Some(w) => w,
                    None => {
                        return Err(err_msg(
                            "Can't upgrade connection that doesn't have a well framed body",
                        ));
                    }
                };

                let reader = DeferredReadable::wrap(reader_waiter.wait());

                return Self::handle_stream_v2(
                    shared,
                    connection_id,
                    Box::new(reader),
                    Box::new(write_stream),
                    ServerConnectionV2Input::Upgrade(req),
                )
                .await;
            }

            // TODO: Apply the transforms here.

            /*
            Check for upgrade that looks like:
                Connection: Upgrade, HTTP2-Settings
                Upgrade: h2c
                HTTP2-Settings: <base64url encoding of HTTP/2 SETTINGS payload>
            */

            // In the case of pipelining, start this in a separate task.

            let req_method = req.head.method.clone();

            let mut res = shared.handler.handle_request(req).await;

            // TODO: Validate that no denylisted headers are given in the response
            // (especially Content-Length)

            res = Self::transform_response(res)?;

            let res_body = encode_response_body_v1(req_method, &mut res.head, res.body);

            if shared.shutting_down.load(Ordering::Relaxed) {
                persist_connection = false;
            }

            crate::headers::connection::append_connection_header(
                persist_connection,
                &mut res.head.headers,
            );

            // TODO: If we do detect multiple aliases to a TcpStream, shutdown the
            // tcpstream explicitly

            // Write the response head
            let mut buf = vec![];
            res.head.serialize(&mut buf)?;
            write_stream.write_all(&buf).await?;

            if let Some(mut body) = res_body {
                write_body(body.as_mut(), &mut write_stream).await?;
            }

            if persist_connection {
                if let Some(reader_waiter) = reader_waiter {
                    read_stream = reader_waiter.wait().await?;
                    continue;
                }
            }

            break;
        }

        Ok(())
    }

    async fn handle_stream_v2(
        shared: &Arc<ServerShared>,
        connection_id: usize,
        reader: Box<dyn Readable>,
        writer: Box<dyn Writeable>,
        input: ServerConnectionV2Input,
    ) -> Result<()> {
        let options = shared.options.connection_options_v2.clone();
        let server_handler = ServerRequestHandlerV2 {
            shared: shared.clone(),
        };
        let conn = v2::Connection::new(options, Some(Box::new(server_handler)));

        let mut initial_state = v2::ConnectionInitialState::raw();

        match input {
            ServerConnectionV2Input::Raw => {}
            ServerConnectionV2Input::Upgrade(req) => {
                conn.receive_upgrade_request(req).await?;

                initial_state.upgrade_payload = Some(Box::new(std::io::Cursor::new(
                    b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: h2c\r\n\r\n" as &'static [u8])));
            }
            ServerConnectionV2Input::SkipPrefaceHead => {
                initial_state.seen_preface_head = true;
            }
        }

        let conn_runner = conn.run(initial_state, reader, writer);

        // Mark this connection as V2 so that we can perform graceful shutdown if
        // needed.
        {
            let mut connection_pool = shared.connection_pool.lock().await;
            let connection = connection_pool.connections.get_mut(&connection_id).unwrap();
            connection.mode = ServerConnectionMode::V2(conn);
        }

        return conn_runner.await;
    }

    // TODO: call me?
    fn transform_request(mut req: Request) -> Result<Request> {
        // Apply Transfer-Encoding stuff to the body.

        // Move the 'Host' header into the

        Ok(req)
    }

    fn transform_response(mut res: Response) -> Result<Response> {
        crate::headers::date::append_current_date(&mut res.head.headers);

        // if if let Some(res.body)

        // if res.head.headers.

        // TODO: Don't allow headers such as 'Connection'

        // TODO: Must always send 'Date' header.
        // TODO: Add 'Server' header

        Ok(res)
    }
}

/// Request handler used by the 'Server' for HTTP 2 connections.
/// It wraps the regular request handler given to the 'Server'
struct ServerRequestHandlerV2 {
    shared: Arc<ServerShared>,
}

#[async_trait]
impl RequestHandler for ServerRequestHandlerV2 {
    async fn handle_request(&self, request: Request) -> Response {
        // TODO: transform request/response
        self.shared.handler.handle_request(request).await
    }
}
