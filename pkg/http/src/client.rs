use std::convert::TryInto;
use std::net::SocketAddr;

use common::async_std::net::TcpStream;
use common::async_std::prelude::*;
use common::errors::*;

use crate::body::*;
use crate::dns::*;
use crate::header::*;
use crate::header_syntax::*;
use crate::message::*;
use crate::message_syntax::*;
use crate::reader::*;
use crate::spec::*;
use crate::status_code::*;
use crate::encoding::*;
use crate::encoding_syntax::*;
use crate::uri::*;
use crate::response::*;
use crate::request::*;
use crate::method::*;

// TODO: Need to clearly document which responsibilities are reserved for the client.

/// HTTP client connected to a single server.
pub struct Client {
    socket_addr: SocketAddr,
}

impl Client {
    /// Creates a new client connecting to the given host/protocol.
    /// NOTE: This will not start a connection.
    pub fn create(uri: &str) -> Result<Client> {
        // TODO: Implement some other form of parser function that doesn't
        // accept anything but the scheme, authority
        let u = uri.parse::<Uri>()?;
        // NOTE: u.path may be '/'
        if !u.path.as_ref().is_empty() || u.query.is_some() || u.fragment.is_some() {
            return Err(err_msg("Can't create a client with a uri path"));
        }

        let scheme = u
            .scheme
            .map(|s| s.to_string())
            .unwrap_or("http".into())
            .to_ascii_lowercase();

        let (default_port, is_secure) = match scheme.as_str() {
            "http" => (80, false),
            "https" => (443, true),
            _ => {
                // TODO: Create an err! macro
                return Err(format_err!("Unsupported scheme {}", scheme));
            }
        };

        if is_secure {
            return Err(err_msg("TLS/SSL currently not supported"));
        }

        let authority = u
            .authority
            .ok_or(err_msg("No authority/hostname specified"))?;

        // TODO: Definately need a more specific type of Uri to ensure that we
        // don't miss any fields.
        if authority.user.is_some() {
            return Err(err_msg("Users not supported"));
        }

        let port = authority.port.unwrap_or(default_port);

        let ip = match authority.host {
            Host::Name(n) => {
                // TODO: This should become async.
                let addrs = lookup_hostname(n.as_ref())?;
                let mut ip = None;
                // TODO: Prefer ipv6 over ipv4?
                for a in addrs {
                    if a.socket_type == SocketType::Stream {
                        ip = Some(a.address);
                        break;
                    }
                }

                match ip {
                    Some(i) => i,
                    None => {
                        return Err(err_msg("Failed to resolve host to an ip"));
                    }
                }
            }
            Host::IP(ip) => ip,
        };

        Ok(Client {
            // TODO: Check port is in u16 range in the parser
            socket_addr: SocketAddr::new(ip.try_into()?, port as u16),
        })
    }

    // TODO: If we recieve an unterminated body, then we should close the
    // connection right afterwards.

    /// Based on the procedure in RFC7230 3.3.3. Message Body Length
    /// Implemented from the client/requester point of view.
    fn create_body(
        req: &Request,
        res_head: &ResponseHead,
        stream: StreamReader,
    ) -> Result<Box<dyn Body>> {
        // 1.
        let code = res_head.status_code.as_u16();
        if req.head.method == Method::HEAD
            || (code >= 100 && code < 200)
            || res_head.status_code == NO_CONTENT
            || res_head.status_code == NOT_MODIFIED
        {
            return Ok(EmptyBody());
        }

        // 2.
        if req.head.method == Method::CONNECT && (code >= 200 && code < 300) {
            return Ok(EmptyBody());
        }

        let transfer_encoding = parse_transfer_encoding(&res_head.headers)?;
        let content_length = parse_content_length(&res_head.headers)?;

        // These should never both be present.
        if transfer_encoding.len() > 0 && content_length.is_some() {
            return Err(err_msg(
                "Messages can not have both a Transfer-Encoding \
								and Content-Length",
            ));
        }

        // 3.
        // NOTE: The length of the transfer_encoding is limited by
        // parse_transfer_encoding already.
        if transfer_encoding.len() > 0 {
            return get_transfer_encoding_body(transfer_encoding, stream);
        }
        /*
        If a Transfer-Encoding header field is present in a response and
       the chunked transfer coding is not the final encoding, the
       message body length is determined by reading the connection until
       it is closed by the server.  If a Transfer-Encoding header field
       is present in a request and the chunked transfer coding is not
       the final encoding, the message body length cannot be determined
       reliably; the server MUST respond with the 400 (Bad Request)
       status code and then close the connection.
        */

        // 4.
        // This is handled by the parse_content_length validation from earlier.

        // 5.
        if let Some(length) = content_length {
            return Ok(Box::new(IncomingSizedBody { stream, length }));
        }

        // 6.
        // Only applicable on the server side

        // 7.
        Ok(Box::new(IncomingUnboundedBody { stream }))
    }

    // Given request, if not connected, connect
    // Write request to stream
    // Read response
    // - TODO: Response may be available before the request is sent (in the case of
    //   bodies)
    // If not using a content length, then we should close the connection
    pub async fn request(&self, mut req: Request) -> Result<Response> {
        if
        /* req.head.headers.has(CONTENT_LENGTH) || */
        req.head.headers.has(KEEP_ALIVE) || req.head.headers.has(TRANSFER_ENCODING) {
            return Err(err_msg("Given reserved header"));
        }

        // TODO: For an empty body, the client doesn't need to send any special headers.

        // TODO: Use timeout?
        let stream = TcpStream::connect(self.socket_addr).await?;
        stream.set_nodelay(true)?;
        
        let mut write_stream = stream.clone();
        let mut read_stream = StreamReader::new(Box::new(stream), MESSAGE_HEAD_BUFFER_OPTIONS);

        // TODO: Set 'Connection: keep-alive' to support talking to legacy (1.0 servers)

        let mut out = vec![];
        req.head.serialize(&mut out);
        write_stream.write_all(&out).await?;
        write_body(req.body.as_mut(), &mut write_stream).await?;

        let head = match read_http_message(&mut read_stream).await? {
            HttpStreamEvent::MessageHead(h) => h,
            // TODO: Handle other bad cases such as too large headers.
            _ => {
                return Err(err_msg("Connection closed without a complete response"));
            }
        };

        let body_start_idx = head.len();

        //		println!("{:?}", String::from_utf8(head.to_vec()).unwrap());

        let msg = match parse_http_message_head(head) {
            Ok((msg, rest)) => {
                assert_eq!(rest.len(), 0);
                msg
            }
            Err(e) => {
                // TODO: Consolidate these lines.
                println!("Failed to parse message\n{}", e);
                return Err(err_msg("Invalid message received"));
            }
        };

        let start_line = msg.start_line;
        let headers = msg.headers;

        // Verify that we got a Request style message
        let status_line = match start_line {
            StartLine::Request(r) => {
                return Err(err_msg("Received a request?"));
            }
            StartLine::Response(r) => r,
        };

        let status_code = StatusCode::from_u16(status_line.status_code)
            .ok_or(Error::from(err_msg("Invalid status code")))?;

        // TODO: If this is a HEAD request, do not receive any body

        // If chunked encoding is used, then it msut b

        // A sender MUST NOT send a Content-Length header field in any message
        //    that contains a Transfer-Encoding header field.

        let head = ResponseHead {
            version: status_line.version,
            // TODO: Print the code in the error case
            status_code,
            reason: status_line.reason,
            headers,
        };

        let body = Self::create_body(&req, &head, read_stream)?;

        Ok(Response { head, body })
    }
}
