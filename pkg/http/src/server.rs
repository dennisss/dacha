use std::convert::TryFrom;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use common::async_std::net::{TcpListener, TcpStream};
use common::async_std::task;
use common::errors::*;
use common::futures::stream::StreamExt;
use common::io::*;
use common::borrowed::{Borrowed, BorrowedReturner};

use crate::{body::*, encoding::decode_transfer_encoding_body};
use crate::header_syntax::*;
use crate::message::*;
use crate::message_syntax::*;
use crate::reader::*;
use crate::spec::*;
use crate::method::*;
use crate::request::*;
use crate::response::*;
use crate::uri::IPAddress;
use crate::status_code::*;
use crate::v2;
use crate::message_body::create_server_request_body;


// TODO: See https://tools.ietf.org/html/rfc7230#section-3.5 for
// robustness tips and accepting empty lines before a request-line.

// TODO: See https://tools.ietf.org/html/rfc7230#section-3.3.3 with
// special HEAD/status code behavior


#[async_trait]
pub trait RequestHandler: Send + Sync {
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

// I can read from a Borrowed<Readable> once it is done.

struct RequestContext {

    pub secure: bool,

    pub peer_addr: IPAddress,

    // TODO: In the future, it will also be useful to have HTTP2 specific information.
}


// TODO: Need to 

/// Receives HTTP requests and parses them.
/// Passes the request to a handler which can produce a response.
pub struct Server {
    port: u16,
    handler: Arc<dyn RequestHandler>,

    // TODO: Maintain a list of all the connections that are currently active?
}

impl Server {
    pub fn new<H: 'static + RequestHandler>(port: u16, handler: H) -> Self {
        Self {
            port,
            handler: Arc::new(handler),
        }
    }

    // TODO: Ideally we'd support using some alternative connection (e.g. a TlsServer)
    pub async fn run(&self) -> Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", self.port)).await?;

        let mut incoming = listener.incoming();
        while let Some(stream) = incoming.next().await {
            let s = stream?;
            task::spawn(Self::handle_stream(s, self.handler.clone()));
        }

        Ok(())
    }

    /*
        TODO: We want to have a way of introspecting a request stream to see things like the client's IP.
    */
    // TODO: Should be refactored to 
    async fn handle_stream(stream: TcpStream, handler: Arc<dyn RequestHandler>) {
        match Self::handle_client(stream, handler).await {
            Ok(v) => {}
            // TODO: If we see a ProtocolErrorV1, form an HTTP 1.1 resposne.
            // A ProtocolErrorV2 should probably also be a 
            Err(e) => println!("Client thread failed: {}", e),
        };
    }

    // TODO: Verify that the HTTP2 error handling works ok.

    // TODO: Errors in here should close the connection.
    async fn handle_client(stream: TcpStream, handler: Arc<dyn RequestHandler>) -> Result<()> {
        let mut write_stream = stream.clone();
        let mut read_stream = PatternReader::new(Box::new(stream), MESSAGE_HEAD_BUFFER_OPTIONS);

        loop {
            let head = match read_http_message(&mut read_stream).await? {
                HttpStreamEvent::MessageHead(h) => h,
                HttpStreamEvent::HeadersTooLarge => {
                    return Err(ProtocolErrorV1 {
                        code: REQUEST_HEADER_FIELDS_TOO_LARGE,
                        message: ""
                    }.into());
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

            // TODO: If we previously negotiated HTTP2, complain if we didn't actually end up using it.

            // TODO: In HTTP2, does the client need to send a headers frame?
            // TODO: "Upon receiving the 101 response, the client MUST send a connection preface (Section 3.5), which includes a SETTINGS frame"

            // Verify supported HTTP version
            match request_line.version {
                HTTP_V0_9 => {}
                HTTP_V1_0 => {}
                HTTP_V1_1 => {}
                HTTP_V2_0 => {
                    // In this case, we received the first two lines of the HTTP 2 connection preface
                    // which should always be "PRI * HTTP/2.0\r\n\r\n"
                    if request_line.method.as_ref() != "PRI" || request_line.target != RequestTarget::AsteriskForm ||
                    !headers.raw_headers.is_empty() {
                        return Err(ProtocolErrorV1 {
                            code: BAD_REQUEST,
                            message: "Interface preface head for HTTP 2.0"
                        }.into());
                    }

                    let server_handler = ServerRequestHandlerV2 { request_handler: handler };
                    let conn = crate::v2::Connection::new(Some(Box::new(server_handler)));

                    let mut initial_state = v2::ConnectionInitialState::raw();
                    initial_state.seen_preface_head = true;

                    // TODO: Record errors.
                    return conn.run(initial_state,Box::new(read_stream), Box::new(write_stream)).await;
                },
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
                    println!("Unsupported http method: {:?}", request_line.method);
                    write_stream
                        .write_all(b"HTTP/1.1 405 Method Not Allowed\r\n\r\n")
                        .await?;
                    return Ok(());
                }
            };

            // TODO:
            // A server MUST respond with a 400 (Bad Request) status code to any
            // HTTP/1.1 request message that lacks a Host header field and to any
            // request message that contains more than one Host header field or a
            // Host header field with an invalid field-value.
            // ^ Do this. Move it into the uri. (unless the URI already has one)

            let request_head = RequestHead {
                method,
                uri: request_line.target.into_uri(),
                version: request_line.version,
                headers,
            };

            // TODO: Convert the error into a response.
            let mut persist_connection = crate::headers::connection::can_connection_persist(
                &request_head.version, &request_head.headers)?;

            let (body, mut reader_waiter) = match create_server_request_body(
                &request_head, read_stream) {
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


            let upgrade_protocols = crate::headers::upgrade_syntax::parse_upgrade(&req.head.headers)?;

            let mut has_h2c_upgrade = false;
            for protocol in &upgrade_protocols {
                if protocol.name.as_ref() == "h2c" && protocol.version.is_none() {
                    has_h2c_upgrade = true;
                    break;
                }
            }

            // Two modes:
            // - Straight away using HTTP2
            // - Upgrade to HTTP2.

            if has_h2c_upgrade {
                // TODO: 

                let reader_waiter = match reader_waiter.take() {
                    Some(w) => w,
                    None => {
                        return Err(err_msg("Can't upgrade connection that doesn't have a well framed body"));
                    }
                };

                // TODO: Initialize the connection with the settings received from the client.
                let server_handler = ServerRequestHandlerV2 { request_handler: handler };
                let conn = v2::Connection::new(Some(Box::new(server_handler)));

                conn.process_upgrade_request(req).await?;

                let reader = DeferredReadable::wrap(reader_waiter.wait());

                // TODO: Refactor to serialize this from a struct
                // TODO: Parallelize the execution of this with the initialization of the connection.
                let mut initial_state = v2::ConnectionInitialState::raw();
                initial_state.upgrade_payload = Some(Box::new(std::io::Cursor::new(
                    b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: h2c\r\n\r\n" as &'static [u8])));

                return conn.run(initial_state, Box::new(reader), Box::new(write_stream)).await;
            }


            // TODO: Apply the transforms here.


            /*
            Check for upgrade that looks like:
                Connection: Upgrade, HTTP2-Settings
                Upgrade: h2c
                HTTP2-Settings: <base64url encoding of HTTP/2 SETTINGS payload>
            */


            // In the case of pipelining, start this in a separate task.

            let mut res = handler.handle_request(req).await;

            res = Self::transform_response(res)?;

            crate::headers::connection::append_connection_header(
                persist_connection, &mut res.head.headers);

            // TODO: If we do detect multiple aliases to a TcpStream, shutdown the
            // tcpstream explicitly

            // let mut res_writer = OutgoingBody { stream: shared_stream.clone() };
            let mut buf = vec![];
            res.head.serialize(&mut buf)?;
            write_stream.write_all(&buf).await?;

            write_body(res.body.as_mut(), &mut write_stream).await?;


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

    fn transform_request(mut req: Request) -> Result<Request> {
        // Apply Transfer-Encoding stuff to the body.
        
        // Move the 'Host' header into the 

        Ok(req)
    }

    fn transform_response(mut res: Response) -> Result<Response> {
        crate::headers::date::append_current_date(&mut res.head.headers);

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
    request_handler: Arc<dyn RequestHandler>
}

#[async_trait]
impl RequestHandler for ServerRequestHandlerV2 {
    async fn handle_request(&self, request: Request) -> Response {
        // TODO: transform request/response
        self.request_handler.handle_request(request).await
    }
}

