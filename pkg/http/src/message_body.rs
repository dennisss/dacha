use common::errors::*;
use common::borrowed::{Borrowed, BorrowedReturner};

use crate::reader::PatternReader;
use crate::body::*;
use crate::chunked::IncomingChunkedBody;
use crate::method::*;
use crate::status_code::*;
use crate::header_syntax::parse_content_length;
use crate::encoding::*;
use crate::encoding_syntax::*;
use crate::request::*;
use crate::response::*;

// TODO: All of this logic should also apply to HTTP 2 with the main exception being that we don't need to
// shard the reader.

/// Based on the procedure in RFC7230 3.3.3. Message Body Length
/// Implemented from the client/requester point of view.
pub fn create_client_response_body(
    req: &Request,
    res_head: &ResponseHead,
    reader: PatternReader,
) -> Result<(Box<dyn Body>, Option<BodyReadCompletion>)> {
    let (reader, reader_returner) = Borrowed::wrap(reader);
    let mut close_delimited = false;

    let body = || -> Result<Box<dyn Body>> {
        // 1.
        let code = res_head.status_code.as_u16();
        if req.head.method == Method::HEAD
            || (code >= 100 && code < 200)
            || res_head.status_code == NO_CONTENT
            || res_head.status_code == NOT_MODIFIED
        {
            close_delimited = false;
            return Ok(EmptyBody());
        }

        // 2.
        if req.head.method == Method::CONNECT && (code >= 200 && code < 300) {
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
                    Box::new(IncomingUnboundedBody { reader })
                }
            };

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
            return Ok(Box::new(IncomingSizedBody::new(reader, length)));
        }

        // 6.
        // Only applicable on the server side

        // 7.
        Ok(Box::new(IncomingUnboundedBody { reader }))
    }()?;

    Ok(wrap_created_body(body, reader_returner, close_delimited))
}

/// Based on the procedure in RFC7230 3.3.3. Message Body Length
/// Implemented from the server/receiver point of view.
///
/// Returns the constructed body and if the body has well defined framing (not
/// connection close terminated), we'll return a future reference to the underlying reader.
///
/// NOTE: Even if the  
pub fn create_server_request_body(
    req_head: &RequestHead, reader: PatternReader
) -> Result<(Box<dyn Body>, Option<BodyReadCompletion>)> {

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

    Ok(wrap_created_body(body, reader_returner, close_delimited))
}

fn wrap_created_body(
    body: Box<dyn Body>, reader_returner: BorrowedReturner<PatternReader>, close_delimited: bool
) -> (Box<dyn Body>, Option<BodyReadCompletion>) {
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
        Some(BodyReadCompletion {
            body_returner,
            reader_returner
        })
    };

    (body, waiter)
}

/// Contains a reference to a Body which may eventually be completely read.
///
/// This allows waiting for the underyling connection stream to become available
/// once the Body was completely read (freeing the connection for usage in sending/receiving
/// other requests/responses).
pub struct BodyReadCompletion {
    body_returner: BorrowedReturner<Box<dyn Body>>,
    reader_returner: BorrowedReturner<PatternReader>
}

impl BodyReadCompletion {
    pub async fn wait(self: Self) -> Result<PatternReader> {
        {
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

            // NOTE: The 'body' will be dropped here.
        }

        let reader = self.reader_returner.await;
        Ok(reader)
    }
}