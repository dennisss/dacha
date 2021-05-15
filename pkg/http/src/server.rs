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
            Err(e) => println!("Client thread failed: {}", e),
        };
    }

    // TODO: Verify that the HTTP2 error handling works ok.

    // TODO: Errors in here should close the connection.
    async fn handle_client(stream: TcpStream, handler: Arc<dyn RequestHandler>) -> Result<()> {
        let mut write_stream = stream.clone();
        let mut read_stream = PatternReader::new(Box::new(stream), MESSAGE_HEAD_BUFFER_OPTIONS);

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
                    write_stream
                        .write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                        .await?;
                    return Ok(());
                }

                // Need to read the "SM\r\n" line.

                // Initialize the connection.

                // Call the request handler (possibly after some request normalization)

                let server_handler = ServerRequestHandlerV2 { request_handler: handler };
                let conn = crate::v2::Connection::new(Some(Box::new(server_handler)));

                // TODO: Record errors.
                return conn.run(Box::new(read_stream), Box::new(write_stream)).await;
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

        let request_head = RequestHead {
            method,
            uri: request_line.target.into_uri(),
            version: request_line.version,
            headers,
        };

        // TODO: Convert the error into a response.
        let mut persistent_connection = crate::headers::connection::can_connection_persistent(
            &request_head.version, &request_head.headers)?;

        let (body, mut reader_waiter) = match Self::create_request_body(
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
            persistent_connection = false;
        }




        // TODO: See https://tools.ietf.org/html/rfc7230#section-3.5 for
        // robustness tips and accepting empty lines before a request-line.

        // TODO: See https://tools.ietf.org/html/rfc7230#section-3.3.3 with
        // special HEAD/status code behavior

        // NOTE: We assume that the body that borrows the reader 

        // TODO: How would I implement a cancelation?


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

            // TODO: Refactor to serialize this from a struct
            // TODO: Parallelize the execution of this with the initialization of the connection.
            write_stream.write_all(b"HTTP/1.1 101 Switching Protocols\r\nConnection: Upgrade\r\nUpgrade: h2c\r\n\r\n").await?;

            // TODO: Initialize the connection with the settings received from the client.
            let server_handler = ServerRequestHandlerV2 { request_handler: handler };
            let conn = crate::v2::Connection::new(Some(Box::new(server_handler)));

            conn.process_upgrade_request(req).await?;

            let reader = DeferredReadable::wrap(reader_waiter.wait());

            return conn.run(Box::new(reader), Box::new(write_stream)).await;
        }


        // TODO: Apply the transforms here.


        /*
        Check for upgrade that looks like:
            Connection: Upgrade, HTTP2-Settings
            Upgrade: h2c
            HTTP2-Settings: <base64url encoding of HTTP/2 SETTINGS payload>
        
        If we do get it, verify that the body has a well defined size.

        Write the 101 upgrading response.

        Give the request handler a borrowed body.

        Start running the HTTP2 writer thread for the connection preface stuff
            And let it know about he 

        Reader thread can also be started but must block on getting the borrowed body back first:
            - Which will need to be read to completion and then downcast to the Readable instance to be fully allowed
            - 

        */

        let mut res = handler.handle_request(req).await;

        res = Self::transform_response(res)?;

        crate::headers::connection::append_connection_header(
            persistent_connection, &mut res.head.headers);

        // TODO: If we do detect multiple aliases to a TcpStream, shutdown the
        // tcpstream explicitly

        // let mut res_writer = OutgoingBody { stream: shared_stream.clone() };
        let mut buf = vec![];
        res.head.serialize(&mut buf);
        write_stream.write_all(&buf).await?;

        write_body(res.body.as_mut(), &mut write_stream).await?;

        Ok(())
    }

    /// Based on the procedure in RFC7230 3.3.3. Message Body Length
    /// Implemented from the server/receiver point of view.
    ///
    /// Returns the constructed body and if the body has well defined framing (not
    /// connection close terminated), we'll return a future reference to the underlying reader.
    ///
    /// NOTE: Even if the  
    fn create_request_body(
        req_head: &RequestHead, reader: PatternReader
    ) -> Result<(Box<dyn Body>, Option<RequestReaderWaiter>)> {

        let (reader, reader_returner) = Borrowed::wrap(reader);

        let mut close_delimited = true;

        // 1-2.
        // Only applicable to a client

        let body = {
            let mut transfer_encoding = crate::encoding_syntax::parse_transfer_encoding(&req_head.headers)?;

            // 3. The Transfer-Encoding header is present (overrides whatever is in Content-Length)
            if transfer_encoding.len() > 0 {
                
                let body = {
                    if transfer_encoding.pop().unwrap().name() == "chunked" {
                        close_delimited = false;
                        Box::new(crate::chunked::IncomingChunkedBody::new(reader))
                    } else {
                        // From the RFC: "If a Transfer-Encoding header field is present in a request and the chunked transfer coding is not the final encoding, the message body length cannot be determined reliably; the server MUST respond with the 400 (Bad Request) status code and then close the connection."
                        return Err(err_msg("Request has unknown length"));
                    }
                };
                
                decode_transfer_encoding_body(transfer_encoding, body)?

            } else {
                // 4. Parsing the Content-Length. Invalid values should close the connection
                let content_length = parse_content_length(&req_head.headers)?;

                if let Some(length) = content_length {
                    // 5.
                    close_delimited = false;
                    Box::new(IncomingSizedBody { reader, length })
                } else {
                    // 6. Empty body!
                    close_delimited = false;
                    crate::body::EmptyBody()
                }
            }
        };

        // 7.
        // Only applicable a client / responses.

        // Construct the returners/waiters.

        // TODO: Instead wrap the body so that when it returns a 0 or Error, we can relinguish the underlying body.
        // (this will usually be much quicker than when we )

        let (body, body_returner) = {
            if body.len() == Some(0) {
                // Optimization for when the body is known to be empty:
                // In this case we don't need to wait for the body to be free'd
                (body, Borrowed::wrap(crate::body::EmptyBody()).1)
            } else {
                let (b, ret) = Borrowed::wrap(body);
                (Box::new(b) as Box<dyn Body>, ret)
            }
        };

        let waiter = if close_delimited { None } else {
            Some(RequestReaderWaiter {
                body_returner,
                reader_returner
            })
        };

        Ok((body, waiter))
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

struct RequestReaderWaiter {
    body_returner: BorrowedReturner<Box<dyn Body>>,
    reader_returner: BorrowedReturner<PatternReader>
}

impl RequestReaderWaiter {
    async fn wait(self: Self) -> Result<PatternReader> {
        let mut body = self.body_returner.await;

        // Discard any unread bytes of the body.
        // If the body was fully read, then this will also detect if the
        // body ended in an error state.
        loop {
            let mut buf = [0u8; 512];
            let n = body.read(&mut buf).await?;
            if n == 0 {
                break;
            }
        }
        
        let reader = self.reader_returner.await;
        Ok(reader)
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

