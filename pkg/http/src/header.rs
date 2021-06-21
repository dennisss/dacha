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

pub const HOST: &'static [u8] = b"Host";

pub const UPGRADE: &'static [u8] = b"Upgrade";

pub const CONTENT_RANGE: &'static [u8] = b"Content-Range";

pub const TE: &'static [u8] = b"TE";

pub const TRAILERS: &'static [u8] = b"Trailers";

pub const ETAG: &'static [u8] = b"ETag";


/// List of headers which are relevant to maintaining the connection at the HTTP transport layer.
///
/// Users of the HTTP client and server libraries in this package are not allowed to specify any
/// of these header names.
const TRANSPORT_LEVEL_HEADERS: &'static [&'static [u8]] = &[
    CONNECTION, CONTENT_LENGTH, HOST, KEEP_ALIVE, TRANSFER_ENCODING, UPGRADE,

    TE, TRAILERS
];

const CONTENT_LEVEL_HEADERS: &'static [&'static [u8]] = &[
    DATE, CONTENT_ENCODING, CONTENT_RANGE, ETAG
];


#[derive(Debug, Clone)]
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

    /// TODO: Make this check contextual. Anything referenced in the 'Connection' header is also transport level. 
    pub fn is_transport_level(&self) -> bool {
        for name in TRANSPORT_LEVEL_HEADERS {
            if name.eq_ignore_ascii_case(self.name.as_str().as_bytes()) {
                return true;
            }
        }

        false
    }

    pub fn is_content_level(&self) -> bool {
        for name in CONTENT_LEVEL_HEADERS {
            if name.eq_ignore_ascii_case(self.name.as_str().as_bytes()) {
                return true;
            }
        }

        false
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
#[derive(Debug, Clone)]
pub struct Headers {
    // TODO: Convert to an ordered multi hash map
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
    pub fn find<'a, 'b>(&'a self, name: &'a [u8]) -> impl Iterator<Item = &'a Header> {
        self.raw_headers
            .iter()
            .filter(move |h| h.name.eq_ignore_case(name))
    }

    // TODO: Change to take an str as names are always Ascii
    pub fn find_one<'a>(&'a self, name: &[u8]) -> Result<&'a Header> {
        // TODO: Deduplicate this with find().
        let mut iter = self.raw_headers
            .iter()
            .filter(move |h| h.name.eq_ignore_case(name));

        let value = iter.next()
            .ok_or_else(|| format_err!("Missing header named: {:?}", name))?;

        if iter.next().is_some() {
            return Err(format_err!("Expected exactly one header named: {:?}", name));
        }
        
        Ok(value)
    }

    pub fn find_mut<'a>(&'a mut self, name: &'a [u8]) -> Option<&'a mut Header> {
        for header in self.raw_headers.iter_mut() {
            if header.name.eq_ignore_case(name) {
                return Some(header);
            }
        }

        None
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
