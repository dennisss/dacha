use std::collections::HashMap;
use std::convert::TryFrom;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use common::io::*;
use executor::cancellation::AlreadyCancelledToken;
use executor::cancellation::CancellationToken;
use executor::channel;
use executor::sync::AsyncMutex;
use executor::sync::PoisonError;
use executor::JoinHandle;
use executor::{lock, lock_async};
use executor_multitask::*;
use net::ip::IPAddress;
use net::tcp::TcpListener;
use net::tcp::TcpStream;

use crate::alpn::*;
use crate::message::*;
use crate::message_body::{decode_request_body_v1, encode_response_body_v1};
use crate::message_syntax::*;
use crate::method::*;
use crate::reader::*;
use crate::request::*;
use crate::response::*;
use crate::server_handler::*;
use crate::spec::*;
use crate::status_code::*;
use crate::v2;

// TODO: See https://tools.ietf.org/html/rfc7230#section-3.5 for
// robustness tips and accepting empty lines before a request-line.

// TODO: See https://tools.ietf.org/html/rfc7230#section-3.3.3 with
// special HEAD/status code behavior

/*
Some more server protections needed:
- Max request deadline (don't respect grpc deadlines from untrusted clients or at least set a hard cap to them)
- Max connection age.
*/

#[derive(Clone)]
pub struct ServerOptions {
    /// What to call this server. Used in resource health tracking reports.
    pub name: String,

    /// Which port to listen to for requests.
    ///
    /// If not set, then a random port will be selected.
    pub port: Option<u16>,

    // TODO: We should make sure that the client uses the "https" scheme
    /// If present, use these options to connect with SSL/TLS. Otherwise, we'll
    /// send requests over plain text.
    pub tls: Option<crypto::tls::ServerOptions>,

    /// If true, we will only accept HTTPv2 connections. Setting this to true
    /// will improve the performance of V2 connections as we will internally
    /// bypass buffering done with V1.
    pub force_http2: bool,

    pub connection_options_v2: v2::ConnectionOptions,

    /// Maximum number of concurrent connections to this server.
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
            name: "HttpServer".to_string(),
            port: None,
            tls: None,
            force_http2: false,
            connection_options_v2: connection_options_v2.clone(),
            max_num_connections: 10000,
            graceful_shutdown_timeout: connection_options_v2.graceful_shutdown_timeout.clone(),
        }
    }
}

/// Receives HTTP requests and parses them.
/// Passes the request to a handler which can produce a response.
pub struct Server {
    shared: Arc<ServerShared>,
}

struct ServerShared {
    handler: Box<dyn ServerHandler>,

    options: ServerOptions,

    connection_pool: AsyncMutex<ServerConnectionPool>,

    // TODO: Make the channels broadcast to all listeners in the case that we call run() multiple
    // times.
    /// An event is sent on this channel whenever we remove a connection from
    /// the connection pool and that causes the pool to become empty.
    ///
    /// This is used during server shutdown to know when we are done shutting
    /// down.
    connection_pool_empty_channel: (channel::Sender<()>, channel::Receiver<()>),

    shutting_down: AtomicBool,

    resource_state: ServiceResourceReportTracker,

    cancellation_tokens: CancellationTokenSet,
}

// TODO: We could possibly improve performance if instead of maintaining a
// connection map, we simply maintain a AtomicUsize with the number of
// connections and have a way to copy and propagate the cancellation token to
// individual connection tasks.
struct ServerConnectionPool {
    connections: HashMap<ServerConnectionId, ServerConnection>,
    last_id: ServerConnectionId,
}

struct ServerConnection {
    /// Handle to the main task that is serving this connection
    task_handle: JoinHandle<()>,

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

pub struct BoundServer {
    shared: Arc<ServerShared>,
    listener: TcpListener,
}

impl BoundServer {
    pub fn start(self) -> ServerResource {
        executor::spawn(Server::run_impl(self.shared.clone(), Some(self.listener)));
        ServerResource {
            shared: self.shared,
        }
    }

    pub fn local_addr(&self) -> Result<net::ip::SocketAddr> {
        self.listener.local_addr()
    }
}

/// Server instance once it has started connection listening threads.
pub struct ServerResource {
    shared: Arc<ServerShared>,
}

#[async_trait]
impl ServiceResource for ServerResource {
    async fn add_cancellation_token(&self, token: Arc<dyn CancellationToken>) {
        self.shared
            .cancellation_tokens
            .add_cancellation_token(token)
            .await
    }

    async fn new_resource_subscriber(&self) -> Box<dyn ServiceResourceSubscriber> {
        self.shared.resource_state.subscribe()
    }
}

// TODO: Standardize this.
impl Drop for ServerResource {
    fn drop(&mut self) {
        let shared = self.shared.clone();
        executor::spawn(async move {
            shared
                .cancellation_tokens
                .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
                .await
        });
    }
}

impl Server {
    pub fn new<H: ServerHandler>(handler: H, mut options: ServerOptions) -> Self {
        if let Some(tls_options) = &mut options.tls {
            tls_options.alpn_ids.push(ALPN_HTTP2.into());
            if !options.force_http2 {
                tls_options.alpn_ids.push(ALPN_HTTP11.into());
            }
        }

        let resource_state = ServiceResourceReportTracker::new(ServiceResourceReport {
            resource_name: options.name.clone(),
            self_state: ServiceResourceState::Loading,
            self_message: None,
            dependencies: vec![],
        });

        Self {
            shared: Arc::new(ServerShared {
                handler: Box::new(handler),
                options,
                connection_pool: AsyncMutex::new(ServerConnectionPool {
                    connections: HashMap::new(),
                    last_id: 0,
                }),
                connection_pool_empty_channel: channel::bounded(1),
                shutting_down: AtomicBool::new(false),
                resource_state,
                cancellation_tokens: CancellationTokenSet::default(),
            }),
        }
    }

    /// TODO: Use a weak pointer?
    async fn run_shutdown_timer(shared: Arc<ServerShared>) {
        executor::sleep(shared.options.graceful_shutdown_timeout).await;
        Self::shutdown_impl(&shared, false).await;
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
    ///
    /// NOT CANCEL SAFE
    ///
    /// TODO: Verify everyone is using the return value of this.
    async fn shutdown_impl(shared: &ServerShared, graceful: bool) -> Result<(), PoisonError> {
        // TODO: Spawn in this in a separate task so that it can't be interrupted.
        lock_async!(connection_pool <= shared.connection_pool.lock().await?, {
            Self::shutdown_impl_inner(shared, graceful, &mut connection_pool).await
        });

        Ok(())
    }

    async fn shutdown_impl_inner(
        shared: &ServerShared,
        graceful: bool,
        connection_pool: &mut ServerConnectionPool,
    ) {
        shared.shutting_down.store(true, Ordering::Relaxed);

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

        // Abrupt cancellation of HTTPv1 connections since there is no other option.
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

    pub async fn bind(mut self) -> Result<BoundServer> {
        let listener = Self::create_listener(&self.shared).await?;

        Ok(BoundServer {
            shared: self.shared,
            listener,
        })
    }

    async fn create_listener(shared: &ServerShared) -> Result<TcpListener> {
        // Bind all all interfaces.
        // TODO: Have an explicit keep-alive period at the TCP level and also eventualyl
        // close the connection.
        TcpListener::bind(format!("0.0.0.0:{}", shared.options.port.unwrap_or(0)).parse()?).await
    }

    /// Starts listening on the given port and processes new connections until
    /// the server is shut down.
    pub fn start(mut self) -> ServerResource {
        executor::spawn(Self::run_impl(self.shared.clone(), None));
        ServerResource {
            shared: self.shared,
        }
    }

    async fn run_impl(shared: Arc<ServerShared>, listener: Option<TcpListener>) {
        let r = Self::run_impl_inner(&shared, listener).await;

        match r {
            Ok(()) => {
                shared
                    .resource_state
                    .update_self(ServiceResourceState::Done, None)
                    .await;
            }
            Err(e) => {
                shared
                    .resource_state
                    .update_self(ServiceResourceState::PermanentFailure, Some(e.to_string()))
                    .await
            }
        }
    }

    async fn run_impl_inner(
        shared: &Arc<ServerShared>,
        listener: Option<TcpListener>,
    ) -> Result<()> {
        let mut listener = match listener {
            Some(v) => v,
            None => Self::create_listener(&shared).await?,
        };

        shared
            .resource_state
            .update_self(ServiceResourceState::Ready, None)
            .await;

        enum Event {
            NextStream(Result<TcpStream>),
            Shutdown,
        }

        let mut shutdown_timer = None;

        loop {
            let next_stream =
                executor::future::map(Box::pin(listener.accept()), |v: Result<TcpStream>| {
                    Event::NextStream(v)
                });

            let event = {
                let shutdown_event = executor::future::map(
                    shared.cancellation_tokens.wait_for_cancellation(),
                    |_| Event::Shutdown,
                );

                executor::future::race(next_stream, shutdown_event).await
            };

            match event {
                Event::NextStream(stream) => {
                    let mut s = stream?;
                    s.set_nodelay(true)?;

                    lock!(connection_pool <= shared.connection_pool.lock().await?, {
                        if connection_pool.connections.len() > shared.options.max_num_connections {
                            eprintln!("[http::Server] Dropping external connection");
                            drop(s);
                            return;
                        }

                        // TODO: Support over usize # of connections by wrapping and checking if
                        // the id is already in the hashmap.
                        let connection_id = connection_pool.last_id + 1;
                        connection_pool.last_id = connection_id;

                        let task_handle =
                            executor::spawn(Self::handle_stream(shared.clone(), connection_id, s));

                        connection_pool.connections.insert(
                            connection_id,
                            ServerConnection {
                                task_handle,
                                mode: ServerConnectionMode::Unknown,
                            },
                        );
                    });
                }
                Event::Shutdown => {
                    Self::shutdown_impl(&shared, true).await?;
                    shutdown_timer =
                        Some(executor::spawn(Self::run_shutdown_timer(shared.clone())));
                    break;
                }
            }
        }

        shared
            .resource_state
            .update_self(ServiceResourceState::Stopping, None)
            .await;

        // TODO: Verify that when we stop accepting connections, any active connections
        // stay active.
        drop(listener);

        // Wait for all connections to die.
        loop {
            let done = lock!(connection_pool <= shared.connection_pool.lock().await?, {
                connection_pool.connections.is_empty()
            });

            if done {
                break;
            }

            let _ = shared.connection_pool_empty_channel.1.recv().await;
        }

        // TODO: Block until all tasks spawned within this server's context are done
        // running.

        Ok(())
    }

    /*
        TODO: We want to have a way of introspecting a request stream to see things like the client's IP.
    */
    // TODO: Should be refactored to
    async fn handle_stream(
        shared: Arc<ServerShared>,
        connection_id: ServerConnectionId,
        stream: TcpStream,
    ) {
        match Self::handle_stream_impl(&shared, connection_id, stream).await {
            Ok(v) => {}
            // TODO: If we see a ProtocolErrorV1, form an HTTP 1.1 response.
            // (but only if generated locally )
            // A ProtocolErrorV2 should probably also be a
            Err(e) => {
                let mut ignore = false;

                // TODO: If we ever add client side errors to this, we will need to ignore it.
                //
                // "Connection reset by peer". This error typically occurs after a client's
                // persistent connection hits an idle timeout.
                //
                //
                // "UnexpectedEof" similarly would happen if the client closes the connection.
                //
                // TODO: Increment a counter whenever we have any type of non-graceful closure
                // like this?
                if let Some(io_error) = e.downcast_ref::<IoError>() {
                    ignore = true;
                }

                if !ignore {
                    println!("[http::Server] Connection thread failed: {}", e)
                }
            }
        };

        // Now that the connection is finished, remove it from the global list.
        lock!(
            connection_pool <= shared.connection_pool.lock().await.unwrap(),
            {
                connection_pool.connections.remove(&connection_id);
                if connection_pool.connections.is_empty() {
                    let _ = shared.connection_pool_empty_channel.0.try_send(());
                }
            }
        );
    }

    // TODO: Verify that the HTTP2 error handling works ok.

    // TODO: Check that the received scheme matches the encrpytion level used.

    // TODO: Errors in here should close the connection.
    async fn handle_stream_impl(
        shared: &Arc<ServerShared>,
        connection_id: ServerConnectionId,
        stream: TcpStream,
    ) -> Result<()> {
        let raw_peer_addr = stream.peer_addr();

        let mut connection_context = ServerConnectionContext {
            id: connection_id,
            peer_addr: raw_peer_addr.ip().clone(),
            peer_port: raw_peer_addr.port(),
            tls: None,
        };

        let (mut read_stream, mut write_stream) = stream.split();

        let mut negotatied_http11 = false;
        let mut negotiated_http2 = false;
        if let Some(tls_options) = &shared.options.tls {
            // NOTE: If the client is sending invalid TLS packets, this will timeout.
            let app = executor::timeout(
                Duration::from_secs(2),
                crypto::tls::Server::connect(read_stream, write_stream, tls_options),
            )
            .await??;
            if let Some(proto) = &app.handshake_summary.selected_alpn_protocol {
                if proto == ALPN_HTTP11.as_bytes() {
                    negotatied_http11 = true;
                } else if proto == ALPN_HTTP2.as_bytes() {
                    negotiated_http2 = true;
                }
            }

            connection_context.tls = Some(app.handshake_summary);
            read_stream = Box::new(app.reader);
            write_stream = Box::new(app.writer);
        }

        if !shared.handler.handle_connection(&connection_context).await {
            return Ok(());
        }

        if shared.options.force_http2 || negotiated_http2 {
            return Self::handle_stream_v2(
                shared,
                connection_context,
                read_stream,
                write_stream,
                ServerConnectionV2Input::Raw,
            )
            .await;
        }

        let mut read_stream = PatternReader::new(read_stream, MESSAGE_HEAD_BUFFER_OPTIONS);

        loop {
            // TODO: Have a 10 second idle timeout for this.
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
                    // TODO: Switch to returning protocol errors.
                    println!("[http::Server] Failed to parse message\n{}", e);
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
                    if negotatied_http11 {
                        return Err(ProtocolErrorV1 {
                            code: BAD_REQUEST,
                            message: "Negotiated HTTP 1.1, but using 2.0",
                        }
                        .into());
                    }

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
                        connection_context,
                        Box::new(read_stream),
                        write_stream,
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

            let accepts_trailers = crate::encoding_syntax::parse_te_for_trailers(&headers)?;

            let mut request_head = RequestHead {
                method,
                uri: request_line.target.into_uri(),
                version: request_line.version,
                headers,
                accepts_trailers,
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

            let (body_sender, body_returner) = channel::unbounded();

            let (body, body_close_delimited) =
                match decode_request_body_v1(&request_head, read_stream, Arc::new(body_sender))
                    .await
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

            if body_close_delimited {
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
                if body_close_delimited {
                    return Err(err_msg(
                        "Can't upgrade connection that doesn't have a well framed body",
                    ));
                }

                let reader = DeferredReadable::wrap(async move {
                    let body = body_returner.recv().await??;
                    body.wait()
                        .await?
                        // NOTE: This error should never occur if body_close_delimited was correct.
                        .ok_or_else(|| err_msg("Unexpected lack of body"))
                });

                return Self::handle_stream_v2(
                    shared,
                    connection_context,
                    Box::new(reader),
                    write_stream,
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

            let req_context = ServerRequestContext {
                connection_context: &connection_context,
            };

            let mut res = shared.handler.handle_request(req, req_context).await;

            // TODO: Validate that no denylisted headers are given in the response
            // (especially Content-Length)

            res = Self::transform_response(res);

            // NOTE: We assume that this uses well defined framing (otherwise we can't
            // persist the connection).
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
                write_body(body.as_mut(), write_stream.as_mut()).await?;
            }

            let returned_body = body_returner.recv().await??;

            if !persist_connection {
                break;
            }

            read_stream = match returned_body.wait().await? {
                Some(v) => v,
                None => break,
            };
        }

        Ok(())
    }

    async fn handle_stream_v2(
        shared: &Arc<ServerShared>,
        connection_context: ServerConnectionContext,
        reader: Box<dyn Readable>,
        writer: Box<dyn SharedWriteable>,
        input: ServerConnectionV2Input,
    ) -> Result<()> {
        let connection_id = connection_context.id;

        let options = shared.options.connection_options_v2.clone();
        let server_options = v2::ServerConnectionOptions {
            connection_context,
            request_handler: Box::new(ServerHandlerWrap {
                shared: shared.clone(),
            }),
        };

        let conn = v2::Connection::new(options, Some(server_options));

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
        lock!(connection_pool <= shared.connection_pool.lock().await?, {
            let connection = connection_pool.connections.get_mut(&connection_id).unwrap();
            connection.mode = ServerConnectionMode::V2(conn);
        });

        return conn_runner.await;
    }

    // TODO: call me?
    fn transform_request(mut req: Request) -> Result<Request> {
        // Apply Transfer-Encoding stuff to the body.

        // Move the 'Host' header into the

        Ok(req)
    }

    fn transform_response(mut res: Response) -> Response {
        crate::headers::date::append_current_date(&mut res.head.headers);

        // if if let Some(res.body)

        // if res.head.headers.

        // TODO: Don't allow headers such as 'Connection'

        // TODO: Must always send 'Date' header.
        // TODO: Add 'Server' header

        res
    }
}

/// Request handler used by the 'Server' for HTTP 2 connections.
/// It wraps the regular request handler given to the 'Server'
struct ServerHandlerWrap {
    shared: Arc<ServerShared>,
}

#[async_trait]
impl ServerHandler for ServerHandlerWrap {
    // NOTE: We do not pass through handle_connection as that should have already
    // been called by the Server struct.

    async fn handle_request<'a>(
        &self,
        request: Request,
        context: ServerRequestContext<'a>,
    ) -> Response {
        // TODO: transform request/response
        let res = self.shared.handler.handle_request(request, context).await;
        Server::transform_response(res)
    }
}
