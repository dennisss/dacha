// TODO: Move this file to the v1 module.

use std::sync::Arc;

use common::borrowed::{Borrowed, BorrowedReturner};
use common::errors::*;
use common::io::Readable;
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::chunked::{IncomingChunkedBody, OutgoingChunkedBody};
use crate::encoding::*;
use crate::encoding_syntax::*;
use crate::header::{Header, CONTENT_LENGTH, TRANSFER_ENCODING};
use crate::header_syntax::parse_content_length;
use crate::method::*;
use crate::reader::PatternReader;
use crate::request::*;
use crate::response::*;
use crate::status_code::*;
use crate::{body::*, Headers};

pub fn encode_response_body_v1(
    request_method: Method,
    res_head: &mut ResponseHead,
    mut body: Box<dyn Body>,
) -> Option<Box<dyn Body>> {
    // 1. NOTE: HEAD case is handled after the Content-Length is set.
    let code = res_head.status_code.as_u16();
    if (code >= 100 && code < 200)
        || res_head.status_code == NO_CONTENT
        || res_head.status_code == NOT_MODIFIED
    {
        return None;
    }

    // 2.
    if request_method == Method::CONNECT && (code >= 200 && code < 300) {
        return None;
    }

    // 3,4,5
    let body_len = body.len();
    if body_len.is_some() && !body.has_trailers() {
        let len = body_len.unwrap();
        res_head.headers.raw_headers.push(Header {
            name: AsciiString::from(CONTENT_LENGTH).unwrap(),
            value: OpaqueString::from(len.to_string()),
        });
    } else {
        res_head.headers.raw_headers.push(Header {
            name: AsciiString::from(TRANSFER_ENCODING).unwrap(),
            value: OpaqueString::from(b"chunked".as_ref()),
        });

        body = Box::new(OutgoingChunkedBody::new(body));
    }

    // 1.
    if request_method == Method::HEAD {
        return None;
    }

    Some(body)
}

/// Based on the procedure in RFC7230 3.3.3. Message Body Length
/// Implemented from the client/requester point of view.
///
/// Returns the fully decoded body and whether or not the body is close
/// delimited (if true no other data can be returned on this connection after
/// the body is fully read).
pub async fn decode_response_body_v1(
    request_method: Method,
    res_head: &ResponseHead,
    reader: PatternReader,
    return_handler: Arc<dyn BodyReturnHandler>,
) -> Result<(Box<dyn Body>, bool)> {
    let (reader, reader_returner) = Borrowed::wrap(reader);
    let mut close_delimited = false;

    let body = || -> Result<Box<dyn Body>> {
        // 1.
        let code = res_head.status_code.as_u16();
        if request_method == Method::HEAD
            || (code >= 100 && code < 200)
            || res_head.status_code == NO_CONTENT
            || res_head.status_code == NOT_MODIFIED
        {
            close_delimited = false;
            return Ok(EmptyBody());
        }

        // 2.
        if request_method == Method::CONNECT && (code >= 200 && code < 300) {
            close_delimited = false;
            return Ok(EmptyBody());
        }

        let mut transfer_encoding = parse_transfer_encoding(&res_head.headers)?;

        // // These should never both be present.
        // if transfer_encoding.len() > 0 && content_length.is_some() {
        //     return Err(err_msg(
        //         "Messages can not have both a Transfer-Encoding \
        // 						and Content-Length",
        //     ));
        // }

        // 3.
        // NOTE: The length of the transfer_encoding is limited by
        // parse_transfer_encoding already.
        if transfer_encoding.len() > 0 {
            let body: Box<dyn Body> = {
                if transfer_encoding.pop().unwrap().name() == "chunked" {
                    close_delimited = false;
                    Box::new(IncomingChunkedBody::new(reader))
                } else {
                    Box::new(IncomingUnboundedBody::new(reader))
                }
            };

            // TODO: Wrap this around the BorrowedBody. Even if a single request has invalid
            // transfer encoding, we can still recover the connection is it is not close
            // delimited.
            return decode_transfer_encoding_body(transfer_encoding, body);
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
        let content_length = parse_content_length(&res_head.headers)?;

        // 5.
        if let Some(length) = content_length {
            close_delimited = false;
            return Ok(Box::new(IncomingSizedBody::new(reader, length)));
        }

        // 6.
        // Only applicable on the server side

        // 7.
        Ok(Box::new(IncomingUnboundedBody::new(reader)))
    }()?;

    Ok((
        wrap_created_body(body, reader_returner, return_handler, close_delimited).await,
        close_delimited,
    ))
}

/// Should run immediately before a request is sent by a client to a server.
/// This will annotate the request head with the appropriate Content-Length and
/// make the body chunked if it has no length.
///
/// TODO: If we don't believe that the server can support at least HTTP 1.1,
/// don't use a chunked body (instead we will need to be able to use a
/// connection closed body).
///
/// TODO: What happens if we send an HTTP 1 server a chunked body. Will it
/// gracefully fail?
///
/// Assumes no Transfer-Encoding has been applied yet.
/// The returned body will always be suitable for persisting an HTTP1
/// conneciton.
pub fn encode_request_body_v1(req_head: &mut RequestHead, body: Box<dyn Body>) -> Box<dyn Body> {
    let body_len = body.len();
    if body_len.is_some() && !body.has_trailers() {
        let len = body_len.unwrap();
        req_head.headers.raw_headers.push(Header {
            name: AsciiString::from(CONTENT_LENGTH).unwrap(),
            value: OpaqueString::from(len.to_string()),
        });
    } else {
        req_head.headers.raw_headers.push(Header {
            name: AsciiString::from(TRANSFER_ENCODING).unwrap(),
            value: OpaqueString::from(b"chunked".as_ref()),
        });

        return Box::new(OutgoingChunkedBody::new(body));
    }

    body
}

/// Based on the procedure in RFC7230 3.3.3. Message Body Length
/// Implemented from the server/receiver point of view.
///
/// Returns the constructed body and if the body has well defined framing (not
/// connection close terminated), we'll return a future reference to the
/// underlying reader.
///
/// NOTE: Even if the  
pub async fn decode_request_body_v1(
    req_head: &RequestHead,
    reader: PatternReader,
    return_handler: Arc<dyn BodyReturnHandler>,
) -> Result<(Box<dyn Body>, bool)> {
    let (reader, reader_returner) = Borrowed::wrap(reader);

    let mut close_delimited = true;

    // 1-2.
    // Only applicable to a client

    let body = {
        let mut transfer_encoding =
            crate::encoding_syntax::parse_transfer_encoding(&req_head.headers)?;

        // 3. The Transfer-Encoding header is present (overrides whatever is in
        // Content-Length)
        if transfer_encoding.len() > 0 {
            let body = {
                if transfer_encoding.pop().unwrap().name() == "chunked" {
                    close_delimited = false;
                    Box::new(crate::chunked::IncomingChunkedBody::new(reader))
                } else {
                    // From the RFC: "If a Transfer-Encoding header field is present in a request
                    // and the chunked transfer coding is not the final encoding, the message body
                    // length cannot be determined reliably; the server MUST respond with the 400
                    // (Bad Request) status code and then close the connection."
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
                Box::new(IncomingSizedBody::new(reader, length))
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

    Ok((
        wrap_created_body(body, reader_returner, return_handler, close_delimited).await,
        close_delimited,
    ))
}

async fn wrap_created_body(
    body: Box<dyn Body>,
    reader_returner: BorrowedReturner<PatternReader>,
    return_handler: Arc<dyn BodyReturnHandler>,
    close_delimited: bool,
) -> Box<dyn Body> {
    // TODO: Instead wrap the body so that when it returns a 0 or Error, we can
    // relinguish the underlying body. (this will usually be much quicker than
    // when we get back the entire body object)

    if body.len() == Some(0) {
        // Optimization for when the body is known to be empty:
        // In this case we don't need to wait for the body to be read.
        return_handler
            .handle_returned_body(Ok(ReturnedBody {
                body: None,
                reader_returner: Some(reader_returner),
            }))
            .await;
        return body;
    }

    Box::new(BorrowedBody::new(
        body,
        if close_delimited {
            None
        } else {
            Some(reader_returner)
        },
        return_handler,
    ))
}

/// An object which gains control over an HTTP1 body/socket once the
/// request/response handler is done reading it (or an error occurs).
#[async_trait]
pub trait BodyReturnHandler: 'static + Send + Sync {
    /// NOTE: Any blocking in this function will block the request/response
    /// handler. This is intentional to ensure that the client/server instance
    /// can mark the connection is failing before retries are performed.
    async fn handle_returned_body(&self, body: Result<ReturnedBody>);
}

#[async_trait]
impl BodyReturnHandler for common::async_std::channel::Sender<Result<ReturnedBody>> {
    async fn handle_returned_body(&self, body: Result<ReturnedBody>) {
        let _ = self.try_send(body);
    }
}

/// Wrapper around an incoming HTTP1 Body which is owned by the client/server
/// connection instance but given to the request/response handler for
/// processing.
///
/// Once the handler is done reading from the Body, the connection instance is
/// returned the underlying Body so that it can continue reading future incoming
/// requests/responses.
pub struct BorrowedBody {
    inner: Option<BorrowedBodyInner>,
    return_handler: Arc<dyn BodyReturnHandler>,
}

struct BorrowedBodyInner {
    body: Box<dyn Body>,
    reader_returner: Option<BorrowedReturner<PatternReader>>,
}

impl BorrowedBody {
    pub fn new(
        body: Box<dyn Body>,
        reader_returner: Option<BorrowedReturner<PatternReader>>,
        return_handler: Arc<dyn BodyReturnHandler>,
    ) -> Self {
        Self {
            inner: Some(BorrowedBodyInner {
                body,
                reader_returner,
            }),
            return_handler,
        }
    }

    async fn call_handler(&mut self, error: Option<Error>) {
        let inner = self.inner.take().unwrap();

        self.return_handler
            .handle_returned_body(match error {
                Some(error) => Err(error),
                None => Ok(ReturnedBody {
                    body: Some(inner.body),
                    reader_returner: inner.reader_returner,
                }),
            })
            .await;
    }
}

impl Drop for BorrowedBody {
    fn drop(&mut self) {
        // TODO: Debup with call_handler.
        if let Some(inner) = self.inner.take() {
            let return_handler = self.return_handler.clone();
            common::async_std::task::spawn(async move {
                return_handler
                    .handle_returned_body(Ok(ReturnedBody {
                        body: Some(inner.body),
                        reader_returner: inner.reader_returner,
                    }))
                    .await;
            });
        }
    }
}

#[async_trait]
impl Readable for BorrowedBody {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| err_msg("Reading body beyond completion"))?;

        match inner.body.read(buf).await {
            Ok(v) => {
                if v == 0 {
                    self.call_handler(None).await;
                }

                Ok(v)
            }
            Err(e) => {
                self.call_handler(Some(e)).await;
                Err(err_msg("Connection failed while reading body"))
            }
        }
    }
}

#[async_trait]
impl Body for BorrowedBody {
    fn len(&self) -> Option<usize> {
        self.inner.as_ref().unwrap().body.len()
    }

    fn has_trailers(&self) -> bool {
        self.inner.as_ref().unwrap().body.has_trailers()
    }

    async fn trailers(&mut self) -> Result<Option<Headers>> {
        let inner = self
            .inner
            .as_mut()
            .ok_or_else(|| err_msg("Reading body beyond completion"))?;

        match inner.body.trailers().await {
            Ok(v) => {
                self.call_handler(None).await;
                Ok(v)
            }
            Err(e) => {
                self.call_handler(Some(e)).await;
                Err(err_msg("Connection failed while reading trailers"))
            }
        }
    }
}

/// Contains a reference to a Body which may eventually be completely read.
///
/// This allows waiting for the underyling connection stream to become available
/// once the Body was completely read (freeing the connection for usage in
/// sending/receiving other requests/responses).
pub struct ReturnedBody {
    /// May be None if the body is empty.
    body: Option<Box<dyn Body>>,

    /// May be None if the body is close delimited.
    reader_returner: Option<BorrowedReturner<PatternReader>>,
}

impl ReturnedBody {
    pub async fn wait(mut self: Self) -> Result<Option<PatternReader>> {
        let reader_returner = match self.reader_returner.take() {
            Some(v) => v,
            // Don't bother reading the remainder of the body if the body is close delimited.
            None => return Ok(None),
        };

        if let Some(mut body) = self.body.take() {
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

            // NOTE: The 'body' will be dropped here and internally relinquish
            // the reader.
        }

        Ok(Some(reader_returner.await))
    }
}
