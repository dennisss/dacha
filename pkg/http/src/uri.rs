use std::convert::TryFrom;
use std::str::FromStr;

use common::bytes::Bytes;
use common::errors::*;
use net::ip::IPAddress;
use parsing::ascii::*;
use parsing::opaque::OpaqueString;

use crate::uri_syntax::*;

// TODO: WE need to support parsing with a base Uri and also removing dot
// segments https://tools.ietf.org/html/rfc3986#section-5.2.4

/// Uniform Resource Indicator
///
/// This struct is also used for storing a URI reference which may be relative
/// and not contain a scheme or authority.
/// NOTE: URLs are a subset of URIs.
#[derive(Debug, Clone, PartialEq)]
pub struct Uri {
    /// e.g. for a URL 'http://localhost', the scheme will be 'http'
    pub scheme: Option<AsciiString>,

    pub authority: Option<Authority>,

    // TODO: Normalize to "/"
    // See RFC 7230 5.7.2.  Transformations
    // See also 2.7.3 for how to do normalization / comparison of http(s) URIs.

    // TODO: Simplify the parsing of this to just the AsciiString and then
    // it can later be interpreted if needed.
    //
    // Main challenge will be that depending on the context, we may expect different grammars.
    pub path: AsciiString,

    /// Portion of the Uri after the '?' (not including the '?').
    /// NOTE: This may still not contain percent encoded
    pub query: Option<AsciiString>,

    // NOTE: This will always be empty for absolute_uri
    pub fragment: Option<AsciiString>,
}

impl Uri {
    pub fn to_string(&self) -> Result<String> {
        let mut out = vec![];
        crate::uri_syntax::serialize_uri(self, &mut out)?;
        let s = String::from_utf8(out)?;
        Ok(s)
    }
}

impl std::str::FromStr for Uri {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let (v, rest) = parse_uri(Bytes::from(s))?;
        if rest.len() != 0 {
            let reststr = String::from_utf8(rest.to_vec()).unwrap();
            return Err(format_err!("Extra bytes after uri: '{}'.", reststr));
        }

        Ok(v)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Authority {
    pub user: Option<OpaqueString>,
    pub host: Host,
    pub port: Option<u16>,
}

impl Authority {
    pub fn to_string(&self) -> Result<String> {
        let mut out = vec![];
        crate::uri_syntax::serialize_authority(self, &mut out)?;
        let s = String::from_utf8(out)?;
        Ok(s)
    }
}

impl TryFrom<&str> for Authority {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self> {
        let (v, _) = parsing::complete(crate::uri_syntax::parse_authority)(value.into())?;
        Ok(v)
    }
}

impl FromStr for Authority {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Self::try_from(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Host {
    Name(String),
    IP(IPAddress),
}

/// The parsed path of the URI broken down into individual segments with
/// any entities decoded.
#[derive(PartialEq, Clone, Debug)]
pub struct UriPath {
    is_absolute: bool,

    segments: Vec<OpaqueString>,
}

impl UriPath {
    pub fn new(is_absolute: bool, segments: &[&str]) -> Self {
        Self {
            is_absolute,
            segments: segments.iter().map(|s| OpaqueString::from(*s)).collect(),
        }
    }

    /// Whether or not the path starts with a '/'
    pub fn is_absolute(&self) -> bool {
        self.is_absolute
    }

    /// Gets the individual segments in the path.
    /// e.g. "/hello/world" has segments ["hello", "world"]
    ///      "/" has segments [""]
    ///      "" has segments []
    pub fn segments(&self) -> &[OpaqueString] {
        &self.segments
    }

    /// Whether or not the path is equivalent to the empty string "".
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

//////////////////

/// NOTE: This is mainly used internally. Users should prefer to use Uri.
#[derive(Debug)]
pub(crate) enum RawUriPath {
    AbEmpty(Vec<OpaqueString>),
    Absolute(Vec<OpaqueString>),
    Rootless(Vec<OpaqueString>),
    Empty,
}

impl RawUriPath {
    pub fn into_path(self) -> UriPath {
        match self {
            RawUriPath::AbEmpty(v) | RawUriPath::Absolute(v) => UriPath {
                is_absolute: true,
                segments: v,
            },
            RawUriPath::Rootless(v) => UriPath {
                is_absolute: false,
                segments: v,
            },
            RawUriPath::Empty => UriPath {
                is_absolute: false,
                segments: vec![],
            },
        }
    }
}

// TODO: What is this used for?
#[derive(Debug)]
pub(crate) enum RawPath {
    PathAbEmpty(Vec<OpaqueString>),
    PathAbsolute(Vec<OpaqueString>),
    PathNoScheme(Vec<OpaqueString>),
    PathRootless(Vec<OpaqueString>),
    PathEmpty,
}
