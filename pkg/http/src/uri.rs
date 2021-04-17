use std::net::{IpAddr, Ipv4Addr};

use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::*;
use parsing::opaque::OpaqueString;

use crate::uri_syntax::*;

// TODO: WE need to support parsing with a base Uri and also removing dot segments
// https://tools.ietf.org/html/rfc3986#section-5.2.4

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
    
    pub path: OpaqueString,
    
    pub query: Option<OpaqueString>,
    
    // NOTE: This will always be empty for absolute_uri
    pub fragment: Option<OpaqueString>,
}

impl Uri {

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

#[derive(Debug, Clone, PartialEq)]
pub struct Authority {
    pub user: Option<OpaqueString>,
    pub host: Host,
    pub port: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Host {
    Name(String),
    IP(IPAddress),
}

#[derive(Debug, Clone, PartialEq)]
pub enum IPAddress {
    V4(Vec<u8>),
    V6(Vec<u8>),
    VFuture(Vec<u8>),
}

impl std::convert::TryFrom<IPAddress> for IpAddr {
    type Error = Error;

    fn try_from(ip: IPAddress) -> Result<Self> {
        Ok(match ip {
            IPAddress::V4(v) => IpAddr::V4(Ipv4Addr::new(v[0], v[1], v[2], v[3])),
            IPAddress::V6(v) => {
                return Err(err_msg("IPV6 not supported"));
                // TODO: This is wrong. Must parse u16's
                // IpAddr::V6(Ipv6Addr::new(v[0], v[1], v[2], v[3],
                // 						 v[4], v[5], v[6], v[7]))
            }
            IPAddress::VFuture(_) => {
                return Err(err_msg("Future ip address not supported"));
            }
        })
    }
}

/// NOTE: This is mainly used internally. Users should prefer to use Uri.
#[derive(Debug)]
pub(crate) enum UriPath {
    AbEmpty(Vec<OpaqueString>),
    Absolute(Vec<OpaqueString>),
    Rootless(Vec<OpaqueString>),
    Empty,
}

impl UriPath {
    // TODO: THe problem with this is that we can't distinguish between '/' and the percent encoded form.
    pub fn to_opaque_string(&self) -> OpaqueString {
        let append_joined = |strs: &[OpaqueString], out: &mut Vec<u8>| {
            for (i, s) in strs.iter().enumerate() {
                out.extend_from_slice(s.as_bytes());
                if i < strs.len() - 1 {
                    out.push(b'/');
                }
            }
        };

        let mut out = vec![];
        match self {
            UriPath::AbEmpty(v) | UriPath::Absolute(v) => {
                out.push(b'/');
                append_joined(v, &mut out);
            },
            UriPath::Rootless(v) => append_joined(v, &mut out),
            UriPath::Empty => {},
        }

        OpaqueString::from(out)
    }
}

// TODO: What is this used for?
#[derive(Debug)]
pub(crate) enum Path {
    PathAbEmpty(Vec<OpaqueString>),
    PathAbsolute(Vec<OpaqueString>),
    PathNoScheme(Vec<OpaqueString>),
    PathRootless(Vec<OpaqueString>),
    PathEmpty,
}

