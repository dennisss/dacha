use common::errors::*;
use common::bytes::*;
use parsing::ascii::*;
use parsing::opaque::OpaqueString;

use crate::message_syntax::*;

pub const CONNECTION: &'static [u8] = b"Connection";

pub const KEEP_ALIVE: &'static [u8] = b"Keep-Alive";

pub const TRANSFER_ENCODING: &'static [u8] = b"Transfer-Encoding";

pub const CONTENT_LENGTH: &'static [u8] = b"Content-Length";

pub const CONTENT_ENCODING: &'static [u8] = b"Content-Encoding";

pub const CONTENT_TYPE: &'static [u8] = b"Content-Type";

pub const DATE: &'static [u8] = b"Date";

#[derive(Debug)]
pub struct Header {
    pub name: AsciiString,
    pub value: OpaqueString,
}

impl Header {
    pub fn new(name: String, value: String) -> Self {
        // TODO: Remove the lack of safety here.
        Self {
            name: unsafe { AsciiString::from_ascii_unchecked(Bytes::from(name)) },
            value: OpaqueString::from(value.as_str()),
        }
    }

    pub fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {
        serialize_header_field(self, out)?;
        out.extend_from_slice(b"\r\n");
        Ok(())
    }
}

pub trait ToHeaderName {
    fn to_header_name(self) -> Result<AsciiString>;
}

impl<T: AsRef<[u8]>> ToHeaderName for T {
    fn to_header_name(self) -> Result<AsciiString> {
        let f = || {
            let s = AsciiString::from(self.as_ref())?;
            // parse_field_name(s.data.clone())?;
            Ok(s)
        };

        f().map_err(|e: Error| format_err!("Invalid header name: {:?}", e))
    }
}

pub trait ToHeaderValue {
    fn to_header_value(self, name: &AsciiString) -> Result<OpaqueString>;
}

impl<T: AsRef<str>> ToHeaderValue for T {
    fn to_header_value(self, name: &AsciiString) -> Result<OpaqueString> {
        let f = || {
            let s = OpaqueString::from(self.as_ref());
            // TODO: Need not do this as it will be done later during serialization anyway.
            // parse_field_content(s.data.clone())?;
            Ok(s)
        };

        f().map_err(|e: Error| {
            format_err!("Invalid value for header {}: {:?}", name.to_string(), e)
        })
    }
}

/// Container for storing many Headers associated with one request/response.
#[derive(Debug)]
pub struct Headers {
    pub raw_headers: Vec<Header>,
}

impl Headers {
    pub fn new() -> Headers {
        Headers {
            raw_headers: vec![],
        }
    }

    pub fn from(raw_headers: Vec<Header>) -> Headers {
        Headers { raw_headers }
    }

    /// Finds all headers matching a given name.
    pub fn find<'a>(&'a self, name: &'a [u8]) -> impl Iterator<Item = &'a Header> {
        self.raw_headers
            .iter()
            .filter(move |h| h.name.eq_ignore_case(name))
    }

    pub fn has(&self, name: &[u8]) -> bool {
        for h in self.raw_headers.iter() {
            if h.name.eq_ignore_case(name) {
                return true;
            }
        }

        false
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) -> Result<()> {
        // TODO: Prefer to serialize the 'Host' header first in requests (according to RFC 7230 5.4)

        for h in &self.raw_headers {
            h.serialize(buf)?;
        }

        Ok(())
    }
}
