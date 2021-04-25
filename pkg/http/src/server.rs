use common::async_std::net::{TcpListener, TcpStream};
use common::async_std::task;
use common::errors::*;
use common::futures::stream::StreamExt;
use common::io::*;
use std::convert::TryFrom;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::body::*;
use crate::header_syntax::*;
use crate::message::*;
use crate::message_syntax::*;
use crate::reader::*;
use crate::spec::*;
use crate::method::*;
use crate::request::*;
use crate::response::*;
use crate::uri::IPAddress;


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
    /// 
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


struct RequestContext {

    pub secure: bool,

    pub peer_addr: IPAddress,

    // TODO: In the future, it will also be useful to have HTTP2 specific information.
}


/// Receives HTTP requests and parses them.
/// Passes the request to a handler which can produce a response.
pub struct Server {
    port: u16,
    handler: Arc<dyn RequestHandler>,
}

impl Server {
    //	pub fn new<F: 'static + Future<Output=Response> + Send,
    //			   H: 'static + Send + Sync + Fn(Request) -> F>(
    //		port: u16, handler: &'static H) -> Server {
    //		let boxed_handler = move |r| -> Pin<Box<dyn Future<Output=Response> + Send>>
    // { 			Box::pin(handler(r))
    //		};
    //
    //		Server { port, handler: Arc::new(boxed_handler) }
    //	}

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
            Err(e) => println!("Client thread failed: {}", e),
        };
    }

    async fn handle_client(stream: TcpStream, handler: Arc<dyn RequestHandler>) -> Result<()> {
        let mut write_stream = stream.clone();
        let mut read_stream = StreamReader::new(Box::new(stream), MESSAGE_HEAD_BUFFER_OPTIONS);

        // Remaining bytes from the last request read.
        // TODO: Start using this?
        // let mut last_remaining = None;

        let head = match read_http_message(&mut read_stream).await? {
            HttpStreamEvent::MessageHead(h) => h,
            HttpStreamEvent::HeadersTooLarge => {
                write_stream
                    .write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\n\r\n")
                    .await?;
                return Ok(());
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

        // Verify supported HTTP version
        match request_line.version {
            HTTP_V0_9 => {}
            HTTP_V1_0 => {}
            HTTP_V1_1 => {}
            // HTTP_V2_0 => {},
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


        // TODO: Extract content-length and transfer-encoding
        // ^ It would be problematic for a request/response to have both

        // TODO: There is a problem if the Content-Length is specified when the body is chunked.
        let content_length = match parse_content_length(&headers) {
            Ok(len) => len,
            Err(e) => {
                println!("{}", e);
                write_stream
                    .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                    .await?;
                return Ok(());
            }
        };

        println!("Content-Length: {:?}", content_length);

        // TODO: See https://tools.ietf.org/html/rfc7230#section-3.5 for
        // robustness tips and accepting empty lines before a request-line.

        // TODO: See https://tools.ietf.org/html/rfc7230#section-3.3.3 with
        // special HEAD/status code behavior

        // TODO: Will definately need to abstract getting a body for a request.
        let body: Box<dyn Body> = match content_length {
            Some(len) => Box::new(IncomingSizedBody {
                stream: read_stream,
                length: len,
            }),
            None => Box::new(IncomingUnboundedBody {
                stream: read_stream,
            }),
        };
        // TODO: Also apply any Transfer-Encoding stuff. If it is chunked, then we need not create an unboudned body, but rather, a chunked body would be better to use.

        // TODO: How would I implement a cancelation?

        

        let req = Request {
            head: RequestHead {
                method,
                uri: request_line.target.into_uri(),
                version: request_line.version,
                headers,
            },
            body,
        };

        let mut res = handler.handle_request(req).await;

        crate::headers::date::append_current_date(&mut res.head.headers);

        // if res.head.headers.

        // TODO: Don't allow headers such as 'Connection'

        // TODO: Must always send 'Date' header.
        // TODO: Add 'Server' header

        // TODO: If we do detect multiple aliases to a TcpStream, shutdown the
        // tcpstream explicitly

        // let mut res_writer = OutgoingBody { stream: shared_stream.clone() };
        let mut buf = vec![];
        res.head.serialize(&mut buf);
        write_stream.write_all(&buf).await?;

        write_body(res.body.as_mut(), &mut write_stream).await?;

        Ok(())
    }
}
