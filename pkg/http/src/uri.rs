use std::convert::TryFrom;
use std::str::FromStr;

use common::bytes::{Buf, Bytes};
use common::errors::*;
use net::ip::IPAddress;
use parsing::ascii::*;
use parsing::opaque::OpaqueString;

use crate::uri_syntax::*;

/// Uniform Resource Indicator
///
/// NOTE: This struct is also used for storing a Uri reference which is a Uri
/// that doesn't have a scheme.
#[derive(Debug, Clone, PartialEq)]
pub struct Uri {
    /// Protocol associated with the Uri.
    ///
    /// - Will be empty only for Uri references.
    /// - Example: For a URL 'http://localhost', the scheme will be 'http'
    pub scheme: Option<AsciiString>,

    pub authority: Option<Authority>,

    /// Path segments (e.g. '/hello/%2dworld').
    ///
    /// Note that percent encoded characters are not decoded in this form.
    /// When there is an authority, this MUST start with '/' or be empty.
    pub path: AsciiString,

    /// Portion of the Uri after the '?' (not including the '?').
    /// NOTE: This may still not contain percent encoded
    pub query: Option<AsciiString>,

    pub fragment: Option<AsciiString>,
}

impl Uri {
    pub fn to_string(&self) -> Result<String> {
        let mut out = vec![];
        crate::uri_syntax::serialize_uri(self, &mut out)?;
        let s = String::from_utf8(out)?;
        Ok(s)
    }

    /// Appends a 'path' to the end of a base uri (self).
    ///
    /// (also normalizes the output uri)
    ///
    /// - 'path' may be either a full uri or a uri reference.
    /// - See https://datatracker.ietf.org/doc/html/rfc3986#section-5.2.2
    pub fn join(&self, relative: &Self) -> Result<Self> {
        if self.scheme.is_none() {
            return Err(err_msg("Can not join to a Uri reference."));
        }

        let mut out = relative.clone();

        if relative.scheme.is_none() {
            out.scheme = self.scheme.clone();

            if relative.authority.is_none() {
                out.authority = self.authority.clone();

                if relative.path.as_str().is_empty() {
                    out.path = self.path.clone();
                }
                // Merge Paths in section 5.2.3 of the RFC
                else if !relative.path.as_str().starts_with("/") {
                    if self.authority.is_some() && self.path.as_str().is_empty() {
                        out.path = AsciiString::new(&format!("/{}", relative.path.as_str()));
                    } else {
                        // Find the position after the last '/'
                        let mut i = self.path.as_str().len();
                        if i > 0 {
                            i -= 1;

                            loop {
                                if self.path.as_str().as_bytes()[i] == b'/' {
                                    i += 1;
                                    break;
                                }

                                if i > 0 {
                                    i -= 1;
                                } else {
                                    break;
                                }
                            }
                        }

                        out.path = AsciiString::new(&format!(
                            "{}{}",
                            self.path.as_str().split_at(i).0,
                            relative.path.as_str()
                        ));
                    }
                }

                if relative.path.as_str().is_empty() && !relative.query.is_some() {
                    out.query = self.query.clone();
                }
            }
        }

        // NOTE: out.fragment is always pulled from 'relative.query'.

        out.normalized()
    }

    /// Normalizes the Uri using safe transformations.
    /// (dot patterns like '.' and '..' will be removed after calling this).
    ///
    /// This requires that self is NOT a Uri reference. References should be
    /// joined with a base Uri before being normalized.
    ///
    /// TODO: Perform normalization before sending or receiving any http request
    /// from a server (this requires that the servers don't see uri references)
    ///
    /// References:
    /// - https://en.wikipedia.org/wiki/URI_normalization
    /// - https://tools.ietf.org/html/rfc3986#section-5.2.4
    pub fn normalized(&self) -> Result<Self> {
        if self.scheme.is_none() {
            return Err(err_msg("Can not normalize a Uri reference."));
        }

        let scheme = self
            .scheme
            .as_ref()
            .map(|s| AsciiString::new(&s.as_str().to_ascii_lowercase()));

        let mut authority = self.authority.clone().map(|mut a| {
            if let Host::Name(n) = &mut a.host {
                *n = n.to_ascii_lowercase().to_string();
            }

            a
        });

        // Parse and re-serialize percent encoded parts of the path.
        let mut path_buf = vec![];
        {
            let mut remaining = self.path.data.clone();

            // When there is an authority, an empty path can be normalized to a '/' path.
            // Also all paths with an authority must start with a '/' to act as a delimiter.
            if authority.is_some() {
                if remaining.is_empty() || remaining[0] != b'/' {
                    path_buf.push(b'/');
                }
            }

            while !remaining.is_empty() {
                if remaining[0] == b'/' {
                    path_buf.push(b'/');
                    remaining.advance(1);
                    continue;
                }

                let (c, r) = parse_pchar(remaining)?;
                remaining = r;
                serialize_pchar(c, &mut path_buf);
            }
        }

        // Remove dot segments.
        // The correctness of the reading and writing from one buffer simultaneously is
        // based on the fact that the output index is always <= than the input index.
        let mut input_index = 0;
        let mut output_index = 0;
        while input_index < path_buf.len() {
            let rest = &path_buf[input_index..];

            // Step 'A' in the RFC.
            if let Some(r) = rest.strip_prefix(b"../").or(rest.strip_prefix(b"./")) {
                input_index += rest.len() - r.len();
                continue;
            }

            // Step 'B' in the RFC.
            {
                if rest.starts_with(b"/./") {
                    input_index += 2; // Advance to the last '/'.
                    continue;
                }

                if rest == b"/." {
                    input_index += 1;
                    path_buf[input_index] = b'/';
                    continue;
                }
            }

            // Step 'C' in the RFC.
            {
                let mut should_pop_segment = false;

                if rest.starts_with(b"/../") {
                    should_pop_segment = true;
                    input_index += 3; // Advance to the last '/'.
                } else if rest == b"/.." {
                    should_pop_segment = true;
                    input_index += 2;
                }

                if should_pop_segment {
                    path_buf[input_index] = b'/';

                    while output_index > 0 {
                        output_index -= 1;
                        if path_buf[output_index] == b'/' {
                            break;
                        }
                    }

                    continue;
                }
            }

            // Step 'D' in the RFC.
            if rest == b".." || rest == b"." {
                break;
            }

            // Step 'E' in the RFC.
            let start_index = input_index;
            while input_index < path_buf.len() {
                if start_index != input_index && path_buf[input_index] == b'/' {
                    break;
                }

                path_buf[output_index] = path_buf[input_index];
                output_index += 1;
                input_index += 1;
            }

            // if let Some(r) =
            // rest.strip_prefix(b"../").or(rest.strip_prefix(b"./")) {
        }
        path_buf.truncate(output_index);

        Ok(Self {
            scheme,
            authority,
            path: AsciiString::from(path_buf).unwrap(),
            query: self.query.clone(),
            fragment: self.fragment.clone(),
        })
    }
}

impl std::str::FromStr for Uri {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let (v, rest) = parse_uri_reference(Bytes::from(s))?;
        if rest.len() != 0 {
            let reststr = String::from_utf8(rest.to_vec()).unwrap();
            return Err(format_err!("Extra bytes after uri '{}': '{}'.", s, reststr));
        }

        Ok(v)
    }
}

impl TryFrom<&str> for Uri {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        value.parse()
    }
}

impl TryFrom<&String> for Uri {
    type Error = Error;

    fn try_from(value: &String) -> Result<Self> {
        value.parse()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_join_test() -> Result<()> {
        let testcases = &[
            ("http://localhost", "/hello", "http://localhost/hello"),
            ("http://localhost", "/", "http://localhost/"),
            (
                "http://localhost/hello/world",
                "family/",
                "http://localhost/hello/family/",
            ),
            (
                "http://apple.com/hello?work#apples",
                "//google.com",
                "http://google.com/",
            ),
            // Test cases from the RFC
            ("http://a/b/c/d;p?q", "g", "http://a/b/c/g"),
            ("http://a/b/c/d;p?q", "./g", "http://a/b/c/g"),
            ("http://a/b/c/d;p?q", "g/", "http://a/b/c/g/"),
            ("http://a/b/c/d;p?q", "/g", "http://a/g"),
            // NOTE: THis is extra normalization compared to the RFC example output.
            ("http://a/b/c/d;p?q", "//g", "http://g/"),
            ("http://a/b/c/d;p?q", "?y", "http://a/b/c/d;p?y"),
            ("http://a/b/c/d;p?q", "g?y", "http://a/b/c/g?y"),
            ("http://a/b/c/d;p?q", "#s", "http://a/b/c/d;p?q#s"),
            ("http://a/b/c/d;p?q", "g#s", "http://a/b/c/g#s"),
            ("http://a/b/c/d;p?q", "g?y#s", "http://a/b/c/g?y#s"),
            ("http://a/b/c/d;p?q", ";x", "http://a/b/c/;x"),
            ("http://a/b/c/d;p?q", "g;x", "http://a/b/c/g;x"),
            ("http://a/b/c/d;p?q", "g;x?y#s", "http://a/b/c/g;x?y#s"),
            ("http://a/b/c/d;p?q", "", "http://a/b/c/d;p?q"),
            ("http://a/b/c/d;p?q", ".", "http://a/b/c/"),
            ("http://a/b/c/d;p?q", "./", "http://a/b/c/"),
            ("http://a/b/c/d;p?q", "..", "http://a/b/"),
            ("http://a/b/c/d;p?q", "../", "http://a/b/"),
            ("http://a/b/c/d;p?q", "../g", "http://a/b/g"),
            ("http://a/b/c/d;p?q", "../..", "http://a/"),
            ("http://a/b/c/d;p?q", "../../", "http://a/"),
            ("http://a/b/c/d;p?q", "../../g", "http://a/g"),
        ];

        for (base, rel, expected) in testcases {
            assert_eq!(
                base.parse::<Uri>()?.join(&rel.parse()?)?.to_string()?,
                *expected
            );
        }

        Ok(())
    }
}
