// Helpers and constants for working with HTTP2 header fields.

use std::convert::TryFrom;

use common::errors::*;
use common::bytes::Bytes;
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::hpack;
use crate::header::{Headers, Header};
use crate::v2::types::*;
use crate::proto::v2::*;
use crate::request::RequestHead;
use crate::response::ResponseHead;
use crate::message::HTTP_V2_0;
use crate::uri::Uri;
use crate::method::Method;

// TODO: Make these all private if possible
pub const METHOD_PSEUDO_HEADER_NAME: &'static str = ":method";
pub const SCHEME_PSEUDO_HEADER_NAME: &'static str = ":scheme";
pub const PATH_PSEUDO_HEADER_NAME: &'static str = ":path";
pub const AUTHORITY_PSEUDO_HEADER_NAME: &'static str = ":authority";

pub const STATUS_PSEUDO_HEADER_NAME: &'static str = ":status";


/// Reads the initial chunk of pseudo headers from the given full chunk of headers.
/// Each pseudo header is passed to pseudo_handler
/// Returns the list of regular headers.
fn process_header_fields<F: FnMut(hpack::HeaderField) -> Result<()>>(
    headers: Vec<hpack::HeaderField>, mut pseudo_handler: F
) -> Result<Headers> {
    let mut done_pseudo_headers = false;

    let mut regular_headers = vec![];

    for header in headers {
        if header.name.to_ascii_lowercase() != header.name {
            return Err(ProtocolError {
                code: ErrorCode::PROTOCOL_ERROR,
                message: "Header name is not lower case",
                local: true
            }.into());
        }

        if header.name.starts_with(b":") {
            if done_pseudo_headers {
                // TODO: Convert to just a stream level error.
                return Err(ProtocolError {
                    code: ErrorCode::PROTOCOL_ERROR,
                    message: "Pseudo headers not at the beginning of the headers block",
                    local: true
                }.into());
            }

            pseudo_handler(header)?;
        } else {
            done_pseudo_headers = true;
            
            regular_headers.push(Header {
                name: AsciiString::from(header.name)?,
                value: OpaqueString::from(header.value)
            });
        }
    }

    Ok(Headers { raw_headers: regular_headers })
}

pub fn process_request_head(headers: Vec<hpack::HeaderField>) -> Result<RequestHead> {
    let mut method = None;
    let mut scheme = None;
    let mut authority = None;
    let mut path = None;

    let regular_headers = process_header_fields(headers, |header| {
        // TODO: Validate no duplicates

        if header.name == METHOD_PSEUDO_HEADER_NAME.as_bytes() {
            method = Some(Method::try_from(header.value.as_ref())
                .map_err(|_| err_msg("Invalid method"))?);
        } else if header.name == SCHEME_PSEUDO_HEADER_NAME.as_bytes() {
            scheme = Some(AsciiString::from(Bytes::from(header.value))?);
        } else if header.name == AUTHORITY_PSEUDO_HEADER_NAME.as_bytes() {
            authority = Some(parsing::complete(crate::uri_syntax::parse_authority)(header.value.into())?.0);
        } else if header.name == PATH_PSEUDO_HEADER_NAME.as_bytes() {
            path = Some(AsciiString::from(Bytes::from(header.value))?);
        } else {
            return Err(err_msg("Received unknown pseudo header"));
            // Error
        }

        Ok(())
    })?;

    let method = method.ok_or(err_msg("Missing method header"))?;

    if method == Method::CONNECT {
        if scheme.is_some() || path.is_some() || authority.is_none() {
            return Err(err_msg("Missing required headers"));
        }
    } else {
        if scheme.is_none() || path.is_none() {
            return Err(err_msg("Missing required headers"));
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

pub fn process_response_head(headers: Vec<hpack::HeaderField>) -> Result<ResponseHead> {
    let mut status = None;
                    
    let regular_headers = process_header_fields(headers, |header| {
        if header.name == STATUS_PSEUDO_HEADER_NAME.as_bytes() {
            // TODO: COnvert to a stream error if this fails.
            status = Some(parsing::complete(crate::message_syntax::parse_status_code)(header.value.into())?.0);
        } else {
            return Err(ProtocolError {
                code: ErrorCode::PROTOCOL_ERROR,
                message: "Unknown pseudo header received",
                local: true
            }.into());
        }

        Ok(())
    })?;

    Ok(ResponseHead {
        version: HTTP_V2_0,
        // TODO: Remove the unwrap
        status_code: crate::status_code::StatusCode::from_u16(status.ok_or(ProtocolError {
            code: ErrorCode::PROTOCOL_ERROR,
            message: "Response missing status header",
            local: true
        })?).unwrap(),
        reason: OpaqueString::new(),
        headers: regular_headers
    })
}
