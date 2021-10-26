use common::bytes::*;
use common::errors::*;
use parsing::ascii::*;
use parsing::opaque::OpaqueString;

use crate::message_syntax::*;

pub const CONNECTION: &'static str = "Connection";

pub const KEEP_ALIVE: &'static str = "Keep-Alive";

pub const TRANSFER_ENCODING: &'static str = "Transfer-Encoding";

pub const CONTENT_LENGTH: &'static str = "Content-Length";

pub const CONTENT_ENCODING: &'static str = "Content-Encoding";

pub const CONTENT_TYPE: &'static str = "Content-Type";

pub const DATE: &'static str = "Date";

pub const HOST: &'static str = "Host";

pub const UPGRADE: &'static str = "Upgrade";

pub const CONTENT_RANGE: &'static str = "Content-Range";

pub const TE: &'static str = "TE";

pub const TRAILERS: &'static str = "Trailers";

pub const ETAG: &'static str = "ETag";

/// List of headers which are relevant to maintaining the connection at the HTTP
/// transport layer.
///
/// Users of the HTTP client and server libraries in this package are not
/// allowed to specify any of these header names.
const TRANSPORT_LEVEL_HEADERS: &'static [&'static str] = &[
    CONNECTION,
    CONTENT_LENGTH,
    HOST,
    KEEP_ALIVE,
    TRANSFER_ENCODING,
    UPGRADE,
    TE,
    TRAILERS,
];

const CONTENT_LEVEL_HEADERS: &'static [&'static str] =
    &[DATE, CONTENT_ENCODING, CONTENT_RANGE, ETAG, CONTENT_TYPE];

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

    /// TODO: Make this check contextual. Anything referenced in the
    /// 'Connection' header is also transport level.
    pub fn is_transport_level(&self) -> bool {
        for name in TRANSPORT_LEVEL_HEADERS {
            if name.eq_ignore_ascii_case(self.name.as_str()) {
                return true;
            }
        }

        false
    }

    pub fn is_content_level(&self) -> bool {
        for name in CONTENT_LEVEL_HEADERS {
            if name.eq_ignore_ascii_case(self.name.as_str()) {
                return true;
            }
        }

        false
    }
}

pub trait ToHeaderName {
    fn to_header_name(self) -> Result<AsciiString>;
}

impl ToHeaderName for AsciiString {
    fn to_header_name(self) -> Result<AsciiString> {
        Ok(self.clone())
    }
}

impl ToHeaderName for Bytes {
    fn to_header_name(self) -> Result<AsciiString> {
        let f = || {
            let s = AsciiString::from(self)?;
            // parse_field_name(s.data.clone())?;
            Ok(s)
        };

        f().map_err(|e: Error| format_err!("Invalid header name: {:?}", e))
    }
}

// These are expansions of
impl ToHeaderName for &[u8] {
    fn to_header_name(self) -> Result<AsciiString> {
        Bytes::from(self).to_header_name()
    }
}
impl ToHeaderName for Vec<u8> {
    fn to_header_name(self) -> Result<AsciiString> {
        Bytes::from(self).to_header_name()
    }
}
impl ToHeaderName for &str {
    fn to_header_name(self) -> Result<AsciiString> {
        Bytes::from(self).to_header_name()
    }
}

pub trait ToHeaderValue {
    fn to_header_value(self, name: &AsciiString) -> Result<OpaqueString>;
}

impl<T: Into<Bytes>> ToHeaderValue for T {
    fn to_header_value(self, name: &AsciiString) -> Result<OpaqueString> {
        let f = || {
            // TODO: The vast majority of standard header types should never

            let s = OpaqueString::from(self);
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
    pub fn find<'a, 'b>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Header> {
        self.raw_headers
            .iter()
            .filter(move |h| h.name.as_str().eq_ignore_ascii_case(name))
    }

    pub fn get_one<'a>(&'a self, name: &str) -> Result<Option<&'a Header>> {
        // TODO: Deduplicate this with find().
        let mut iter = self
            .raw_headers
            .iter()
            .filter(move |h| h.name.as_str().eq_ignore_ascii_case(name));

        let value = iter.next();

        if value.is_some() && iter.next().is_some() {
            return Err(format_err!("Expected exactly one header named: {:?}", name));
        }

        Ok(value)
    }

    // TODO: Change to take an str as names are always Ascii
    pub fn find_one<'a>(&'a self, name: &str) -> Result<&'a Header> {
        self.get_one(name)?
            .ok_or_else(|| format_err!("Missing header named: {:?}", name))
    }

    pub fn find_mut<'a>(&'a mut self, name: &'a str) -> Option<&'a mut Header> {
        for header in self.raw_headers.iter_mut() {
            if header.name.as_str().eq_ignore_ascii_case(name) {
                return Some(header);
            }
        }

        None
    }

    pub fn has(&self, name: &str) -> bool {
        for h in self.raw_headers.iter() {
            if h.name.as_str().eq_ignore_ascii_case(name) {
                return true;
            }
        }

        false
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) -> Result<()> {
        // TODO: Prefer to serialize the 'Host' header first in requests (according to
        // RFC 7230 5.4)

        for h in &self.raw_headers {
            h.serialize(buf)?;
        }

        Ok(())
    }
}
