use automata::regexp::vm::instance::RegExpMatch;
use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::AsciiString;
use parsing::complete;

use crate::body::*;
use crate::header::*;
use crate::message::*;
use crate::message_syntax::*;
use crate::method::*;
use crate::uri::*;

pub struct Request {
    pub head: RequestHead,
    pub body: Box<dyn Body>,
    // TODO: trailers (Option<Receiver<Result<_>>>) (also add the same thing tot the Response)
}

// TODO: Instead just implement for head (or add some length info to describe
// the body)?
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.head.fmt(f)
    }
}

#[derive(Debug)]
pub struct RequestHead {
    // TODO: Only certain types of URIs are valid in this context
    pub method: Method,
    pub uri: Uri,
    pub version: Version,
    pub headers: Headers,
}

impl RequestHead {
    pub fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {
        serialize_request_line(
            &AsciiString::from(self.method.as_str())?,
            &self.uri,
            &self.version,
            out,
        )?;

        self.headers.serialize(out)?;
        out.extend_from_slice(b"\r\n");
        Ok(())
    }
}

/// Helper for creating
pub struct RequestBuilder {
    method: Option<Method>,
    uri: Option<Uri>,
    headers: Vec<Header>,
    body: Option<Box<dyn Body>>,

    // First error that occured in the building process
    error: Option<Error>,
}

impl RequestBuilder {
    pub fn new() -> RequestBuilder {
        RequestBuilder {
            method: None,
            uri: None,
            headers: vec![],
            error: None,
            body: None,
        }
    }

    pub fn method(mut self, method: Method) -> Self {
        self.method = Some(method);
        self
    }

    pub fn path<P: AsRef<[u8]>>(mut self, path: P) -> Self {
        match simple_path_parser(path.as_ref()) {
            Some((path, query)) => {
                if self.uri.is_some() {
                    self.error = Some(err_msg("Part of the uri is already set"));
                    return self;
                }

                self.uri = Some(Uri {
                    scheme: None,
                    authority: None,
                    path,
                    query,
                    fragment: None,
                });
            }
            None => {
                self.error = Some(err_msg("Invalid path given"));
            }
        }

        // TODO
        self
    }

    pub fn uri(mut self, uri: Uri) -> Self {
        self.uri = Some(uri);
        self
    }

    // // TODO: Use a different parsing rule for this?
    // // We should allow either relative or absolute URIs (or things like '*').
    // // When an absolute Uri is given, we should move the authority to the 'Host'
    // header. // Schemes other than 'http(s)' should be rejected unless using
    // HTTP2 or in a proxy connect mode. pub fn uri<U: AsRef<[u8]>>(mut self,
    // uri: U) -> Self {     // TODO: Implement a complete() parser combinator
    // to deal with ensuring nothing     // is left
    //     self.uri = match
    // complete(parse_request_target)(Bytes::from(uri.as_ref())) {
    //         Ok((u, _)) => Some(u.into_uri()),
    //         Err(e) => {
    //             self.error = Some(format_err!("Invalid request uri: {:?}", e));
    //             None
    //         }
    //     };

    //     self
    // }

    // TODO: Currently this is the only safe way to build a request
    // we will need to dedup this with
    pub fn header<N: ToHeaderName, V: ToHeaderValue>(mut self, name: N, value: V) -> Self {
        let name = match name.to_header_name() {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e);
                return self;
            }
        };

        // TODO: This will never fail?
        let value = match value.to_header_value(&name) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e);
                return self;
            }
        };

        self.headers.push(Header { name, value });
        self
    }

    pub fn body(mut self, body: Box<dyn Body>) -> Self {
        self.body = Some(body);
        self
    }

    /// Constructs the request from the previously provided value.
    ///
    /// NOTE: Even if this succeeds, then the request may still be invalid and
    /// this will only be caught when you attempt to serialize/run the
    /// request.
    pub fn build(self) -> Result<Request> {
        if let Some(e) = self.error {
            return Err(e);
        }

        let method = self.method.ok_or_else(|| err_msg("No method specified"))?;

        // TODO: Only certain types of uris are allowed here
        let uri = self
            .uri
            .ok_or_else(|| err_msg("No uri components specified"))?;

        let headers = Headers::from(self.headers);

        let body = self.body.unwrap_or_else(|| crate::body::EmptyBody());

        Ok(Request {
            head: RequestHead {
                method,
                uri,
                version: HTTP_V1_1,
                headers,
            },
            body,
        })
    }
}

// Simple regular expression for matching a relative path allowed in a request.
// This is allowed to contain an absolute path followed by an optional query
// string. Or the entire string can be '*'.
// For simplicity, we don't validate that percent encoded entities are correct.
regexp!(REQUEST_PATH => "^(?:(\\*)|(/(?:[a-zA-Z0-9-._~!$&'()*+,;=:@%]+/?)*)(?:\\?([A-Za-z0-9-._~!$&'()*+,;=:@%?]*))?)$");

fn simple_path_parser(path: &[u8]) -> Option<(AsciiString, Option<AsciiString>)> {
    let m = match REQUEST_PATH.exec(path) {
        Some(m) => m,
        None => {
            return None;
        }
    };

    // Either take the '*' or the '/...' form of the path.
    let path = AsciiString::from(m.group(1).unwrap_or_else(|| m.group(2).unwrap())).unwrap();

    let query = m.group(3).map(|v| AsciiString::from(v).unwrap());

    Some((path, query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_parser_test() {
        assert_eq!(
            simple_path_parser(b"/"),
            Some((AsciiString::from("/").unwrap(), None))
        );
        assert_eq!(simple_path_parser(b"//"), None);
        assert_eq!(
            simple_path_parser(b"/hello"),
            Some((AsciiString::from("/hello").unwrap(), None))
        );
        assert_eq!(simple_path_parser(b""), None);
        assert_eq!(
            simple_path_parser(b"/hello/world"),
            Some((AsciiString::from("/hello/world").unwrap(), None))
        );
        assert_eq!(
            simple_path_parser(b"/hello/?world=?"),
            Some((
                AsciiString::from("/hello/").unwrap(),
                Some(AsciiString::from("world=?").unwrap())
            ))
        );
        assert_eq!(
            simple_path_parser(b"/hello/?"),
            Some((
                AsciiString::from("/hello/").unwrap(),
                Some(AsciiString::from("").unwrap())
            ))
        );
        assert_eq!(
            simple_path_parser(b"/hello?"),
            Some((
                AsciiString::from("/hello").unwrap(),
                Some(AsciiString::from("").unwrap())
            ))
        );
        assert_eq!(
            simple_path_parser(b"/?"),
            Some((
                AsciiString::from("/").unwrap(),
                Some(AsciiString::from("").unwrap())
            ))
        );
        assert_eq!(simple_path_parser(b"?"), None);
        assert_eq!(simple_path_parser(b"?a"), None);
        assert_eq!(
            simple_path_parser(b"*"),
            Some((AsciiString::from("*").unwrap(), None))
        );

        println!("SIZE: {}", REQUEST_PATH.estimated_memory_usage());
    }
}
