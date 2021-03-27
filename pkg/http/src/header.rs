use common::errors::*;
use common::bytes::*;
use parsing::ascii::*;
use parsing::iso::*;

use crate::message_parser::*;

pub const CONNECTION: &'static [u8] = b"Connection";

pub const KEEP_ALIVE: &'static [u8] = b"Keep-Alive";

pub const TRANSFER_ENCODING: &'static [u8] = b"Transfer-Encoding";

pub const CONTENT_LENGTH: &'static [u8] = b"Content-Length";

pub const CONTENT_ENCODING: &'static [u8] = b"Content-Encoding";

pub const CONTENT_TYPE: &'static [u8] = b"Content-Type";

#[derive(Debug)]
pub struct HttpHeader {
    pub name: AsciiString,
    pub value: Latin1String,
}

impl HttpHeader {
    pub fn new(name: String, value: String) -> Self {
        Self {
            name: unsafe { AsciiString::from_ascii_unchecked(Bytes::from(name)) },
            value: Latin1String::from_bytes(Bytes::from(value)).unwrap(),
        }
    }

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.name.data);
        buf.extend_from_slice(b": ");
        // TODO: Need to better sanity check the value.
        buf.extend_from_slice(&self.value.data);
        buf.extend_from_slice(b"\r\n");
    }
}

pub trait ToHeaderName {
    fn to_header_name(self) -> Result<AsciiString>;
}

impl<T: AsRef<[u8]>> ToHeaderName for T {
    fn to_header_name(self) -> Result<AsciiString> {
        let f = || {
            let s = AsciiString::from(self.as_ref())?;
            parse_field_name(s.data.clone())?;
            Ok(s)
        };

        f().map_err(|e: Error| format_err!("Invalid header name: {:?}", e))
    }
}

pub trait ToHeaderValue {
    fn to_header_value(self, name: &AsciiString) -> Result<Latin1String>;
}

impl<T: AsRef<str>> ToHeaderValue for T {
    fn to_header_value(self, name: &AsciiString) -> Result<Latin1String> {
        let f = || {
            let s = Latin1String::from(self.as_ref())?;
            parse_field_content(s.data.clone())?;
            Ok(s)
        };

        f().map_err(|e: Error| {
            format_err!("Invalid value for header {}: {:?}", name.to_string(), e)
        })
    }
}

/// Container for storing many HttpHeaders associated with one request/response.
#[derive(Debug)]
pub struct HttpHeaders {
    pub raw_headers: Vec<HttpHeader>,
}

impl HttpHeaders {
    pub fn new() -> HttpHeaders {
        HttpHeaders {
            raw_headers: vec![],
        }
    }

    pub fn from(raw_headers: Vec<HttpHeader>) -> HttpHeaders {
        HttpHeaders { raw_headers }
    }

    /// Finds all headers matching a given name.
    pub fn find<'a>(&'a self, name: &'a [u8]) -> impl Iterator<Item = &'a HttpHeader> {
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

    pub fn serialize(&self, buf: &mut Vec<u8>) {
        for h in &self.raw_headers {
            h.serialize(buf);
        }
    }
}
