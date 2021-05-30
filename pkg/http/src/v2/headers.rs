// Helpers and constants for working with HTTP2 header fields.

use std::convert::TryFrom;

use common::io::Writeable;
use common::errors::*;
use common::bytes::Bytes;
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::hpack;
use crate::hpack::HeaderFieldRef;
use crate::header::{Headers, Header};
use crate::v2::types::*;
use crate::proto::v2::*;
use crate::request::RequestHead;
use crate::response::ResponseHead;
use crate::message::HTTP_V2_0;
use crate::uri::Uri;
use crate::method::Method;
use crate::uri_syntax::serialize_authority;

// Request pseudo headers
const METHOD_PSEUDO_HEADER_NAME: &'static str = ":method";
const SCHEME_PSEUDO_HEADER_NAME: &'static str = ":scheme";
const PATH_PSEUDO_HEADER_NAME: &'static str = ":path";
const AUTHORITY_PSEUDO_HEADER_NAME: &'static str = ":authority";

// Response pseudo headers
const STATUS_PSEUDO_HEADER_NAME: &'static str = ":status";


fn is_ascii_lowercase(s: &str) -> bool {
    for c in s.chars() {
        if c.is_ascii_alphabetic() && !c.is_ascii_lowercase() {
            return false;
        }
    }

    true
}

fn to_ascii_string(data: Vec<u8>) -> StreamResult<AsciiString> {
    Ok(AsciiString::from_bytes(Bytes::from(data))
        .map_err(|_| StreamError::malformed_message("Received non ASCII header name/value"))?)   
}


/// Reads the initial chunk of pseudo headers from the given full chunk of headers.
/// Each pseudo header is passed to pseudo_handler
/// Returns the list of regular headers.
fn process_header_fields<F: FnMut(hpack::HeaderField) -> StreamResult<()>>(
    headers: Vec<hpack::HeaderField>, mut pseudo_handler: F
) -> StreamResult<Headers> {
    let mut done_pseudo_headers = false;

    let mut regular_headers = vec![];

    for header in headers {
        if header.name.to_ascii_lowercase() != header.name {
            return Err(StreamError::malformed_message("Header name is not lower case"));
        }

        if header.name.starts_with(b":") {
            if done_pseudo_headers {
                // Receiving regular headers before pseudo headers is invalid. 
                // See RFC 7540: Section 8.1.2.1
                return Err(StreamError::malformed_message(
                    "Pseudo headers not at the beginning of the headers block"));
            }

            pseudo_handler(header)?;
        } else {
            done_pseudo_headers = true;
            
            regular_headers.push(Header {
                name: to_ascii_string(header.name)?,
                value: OpaqueString::from(header.value)
            });
        }
    }

    Ok(Headers { raw_headers: regular_headers })
}

pub fn encode_request_headers_block(head: &RequestHead, encoder: &mut hpack::Encoder) -> Result<Vec<u8>> {
    let mut header_block = vec![];

    encoder.append(HeaderFieldRef {
        name: METHOD_PSEUDO_HEADER_NAME.as_bytes(),
        value: head.method.as_str().as_bytes()
    }, &mut header_block);

    if let Some(scheme) = &head.uri.scheme {
        encoder.append(HeaderFieldRef {
            name: SCHEME_PSEUDO_HEADER_NAME.as_bytes(),
            value: scheme.as_ref().as_bytes(),
        }, &mut header_block);
    }

    // TODO: Ensure that the path is always '/' instead of empty (this should apply to HTTP1 as well).
    // Basically we should always normalize it to '/' when parsing a path.
    {
        let mut path = head.uri.path.as_ref().to_string();
        // TODO: For this we'd need to validate that 'path' doesn't have a '?'
        if let Some(query) = &head.uri.query {
            path.push('?');
            path.push_str(query.as_ref());
        }
        encoder.append(HeaderFieldRef {
            name: PATH_PSEUDO_HEADER_NAME.as_bytes(),
            value: path.as_bytes()
        }, &mut header_block);
    }

    if let Some(authority) = &head.uri.authority {
        let mut authority_value = vec![];
        serialize_authority(authority, &mut authority_value)?;
        
        encoder.append(HeaderFieldRef {
            name: AUTHORITY_PSEUDO_HEADER_NAME.as_bytes(),
            value: &authority_value
        }, &mut header_block);
    }

    for header in head.headers.raw_headers.iter() {
        // TODO: Verify that it doesn't start with a ':'
        let name = header.name.as_ref().to_ascii_lowercase();
        encoder.append(HeaderFieldRef {
            name: name.as_bytes(),
            value: header.value.as_bytes()
        }, &mut header_block);
    }

    Ok(header_block)
}

// TODO: Perform Cookie field compression (also for requests)
pub fn encode_response_headers_block(head: &ResponseHead, encoder: &mut hpack::Encoder) -> Result<Vec<u8>> {
    let mut header_block = vec![];

    encoder.append(HeaderFieldRef {
        name: STATUS_PSEUDO_HEADER_NAME.as_bytes(),
        value: head.status_code.as_u16().to_string().as_bytes()
    }, &mut header_block);

    for header in head.headers.raw_headers.iter() {
        // TODO: Verify that it doesn't start with a ':'
        let name = header.name.as_ref().to_ascii_lowercase();
        encoder.append(HeaderFieldRef {
            name: name.as_bytes(),
            value: header.value.as_bytes()
        }, &mut header_block);
    }

    Ok(header_block)
}

pub fn encode_trailers_block(headers: &Headers, encoder: &mut hpack::Encoder) -> Vec<u8> {
    let mut header_block = vec![];

    for header in headers.raw_headers.iter() {
        // TODO: Verify that no headers start with ':'
        let name = header.name.as_ref().to_ascii_lowercase();
        encoder.append(HeaderFieldRef {
            name: name.as_bytes(),
            value: header.value.as_bytes()
        }, &mut header_block);
    }

    header_block
}

pub fn process_request_head(headers: Vec<hpack::HeaderField>) -> StreamResult<RequestHead> {
    let mut method = None;
    let mut scheme = None;
    let mut authority = None;
    let mut path = None;

    let regular_headers = process_header_fields(headers, |header| {
        // TODO: Validate no duplicates

        if header.name == METHOD_PSEUDO_HEADER_NAME.as_bytes() {
            method = Some(Method::try_from(header.value.as_ref())
                .map_err(|_| StreamError::malformed_message("Invalid method"))?);
        } else if header.name == SCHEME_PSEUDO_HEADER_NAME.as_bytes() {
            scheme = Some(to_ascii_string(header.value)?);
        } else if header.name == AUTHORITY_PSEUDO_HEADER_NAME.as_bytes() {
            authority = Some(parsing::complete(crate::uri_syntax::parse_authority)(header.value.into())
                .map_err(|_| StreamError::malformed_message("Received malformed authority header"))?.0);
        } else if header.name == PATH_PSEUDO_HEADER_NAME.as_bytes() {
            path = Some(to_ascii_string(header.value)?);
        } else {
            return Err(StreamError::malformed_message("Received unknown pseudo header"));
            // Error
        }

        Ok(())
    })?;

    let method = method.ok_or(StreamError::malformed_message("Missing method header"))?;

    if method == Method::CONNECT {
        if scheme.is_some() || path.is_some() || authority.is_none() {
            return Err(StreamError::malformed_message("Missing required headers"));
        }
    } else {
        if scheme.is_none() || path.is_none() {
            return Err(StreamError::malformed_message("Missing required headers"));
        }
    }

    let mut path = path.unwrap_or(AsciiString::from_str("").unwrap());
    let mut query = None;
    if let Some(idx) = path.as_ref().find('?') {
        query = Some(AsciiString::from(&path.as_ref().as_bytes()[(idx + 1)..]).unwrap());
        path = AsciiString::from(&path.data[0..idx]).unwrap();
    }

    Ok(RequestHead {
        method,
        uri: Uri {
            scheme,
            authority,
            path,
            query,
            fragment: None
        },
        version: HTTP_V2_0,
        headers: regular_headers
    })
}

pub fn process_response_head(headers: Vec<hpack::HeaderField>) -> StreamResult<ResponseHead> {
    let mut status = None;
                    
    let regular_headers = process_header_fields(headers, |header| {
        if header.name == STATUS_PSEUDO_HEADER_NAME.as_bytes() {
            // TODO: COnvert to a stream error if this fails.
            status = Some(parsing::complete(crate::message_syntax::parse_status_code)(header.value.into())
                .map_err(|_| StreamError::malformed_message("Received malformed status code"))?.0);
        } else {
            return Err(StreamError::malformed_message("Unknown pseudo header received"));
        }

        Ok(())
    })?;

    Ok(ResponseHead {
        version: HTTP_V2_0,
        // TODO: Remove the unwrap
        status_code: crate::status_code::StatusCode::from_u16(status
            .ok_or(StreamError::malformed_message("Response missing status header"))?)
            .ok_or(StreamError::malformed_message("Response contains out of range status code"))?,
        reason: OpaqueString::new(),
        headers: regular_headers
    })
} 

pub fn process_trailers(headers: Vec<hpack::HeaderField>) -> StreamResult<Headers> {
    let mut out = vec![];
    out.reserve_exact(headers.len());
    
    for header in headers {
        let name = AsciiString::from(header.name)
            .map_err(|_| StreamError(ProtocolErrorV2 {
                code: ErrorCode::PROTOCOL_ERROR,
                message: "Received non-ASCII header name",
                local: true
            }))?;

        if !is_ascii_lowercase(name.as_ref()) {
            return Err(StreamError(ProtocolErrorV2 {
                code: ErrorCode::PROTOCOL_ERROR,
                message: "Received non-lowercase header name",
                local: true
            }));
        }

        if name.as_ref().starts_with(":") {
            return Err(StreamError(ProtocolErrorV2 {
                code: ErrorCode::PROTOCOL_ERROR,
                message: "Received pseudo-header in trailers",
                local: true
            }));
        }

        out.push(Header {
            name,
            value: OpaqueString::from(header.value)
        });
    }

    Ok(Headers::from(out))
}

/// Writes a block of headers in one or more frames.
pub async fn write_headers_block(
    writer: &mut dyn Writeable, stream_id: StreamId, header_block: &[u8], end_stream: bool,
    max_remote_frame_size: usize
) -> Result<()> {
    let mut remaining: &[u8] = &header_block;
    if remaining.len() == 0 {
        return Err(err_msg("For some reason the header block is empty?"));
    }

    let mut first = true;
    while remaining.len() > 0 || first {
        // TODO: Make this more robust. Currently this assumes that we don't include any padding or
        // priority information which means that the entire payload is for the header fragment.
        let n = std::cmp::min(remaining.len(), max_remote_frame_size);
        let end_headers = n == remaining.len();

        let mut frame = vec![];
        if first {
            FrameHeader {
                typ: FrameType::HEADERS,
                length: n as u32,
                flags: HeadersFrameFlags {
                    priority: false,
                    padded: false,
                    end_headers,
                    end_stream,
                    reserved67: 0,
                    reserved4: 0,
                    reserved1: 0,
                }.to_u8().unwrap(),
                stream_id,
                reserved: 0
            }.serialize(&mut frame)?;
            first = false;
        } else {
            FrameHeader {
                typ: FrameType::CONTINUATION,
                length: n as u32,
                flags: ContinuationFrameFlags {
                    end_headers,
                    reserved34567: 0,
                    reserved01: 0
                }.to_u8().unwrap(),
                stream_id,
                reserved: 0
            }.serialize(&mut frame)?;
        }

        frame.extend_from_slice(&remaining[0..n]);
        remaining = &remaining[n..];

        writer.write_all(&frame).await?;
    }

    Ok(())
}
