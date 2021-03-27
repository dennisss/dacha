use common::errors::*;
use common::bytes::Bytes;
use parsing::complete;
use crate::body::*;
use crate::header::*;
use crate::method::*;
use crate::message::*;
use crate::message_parser::*;
use crate::uri::*;

pub struct Request {
    pub head: RequestHead,
    pub body: Box<dyn Body>,
}

#[derive(Debug)]
pub struct RequestHead {
    // TODO: Only certain types of URIs are valid in this context
    pub method: Method,
    pub uri: Uri,
    pub version: HttpVersion,
    pub headers: HttpHeaders,
}

impl RequestHead {
    pub fn serialize(&self, buf: &mut Vec<u8>) {
        let request_line = format!(
            "{} {} HTTP/{}\r\n",
            std::str::from_utf8(self.method.as_str()).unwrap(),
            self.uri.to_string(),
            self.version.to_string()
        );
        buf.extend_from_slice(request_line.as_bytes());

        self.headers.serialize(buf);
        buf.extend_from_slice(b"\r\n");
    }
}

// TODO: Instead just implement for head (or add some length info to describe
// the body)?
impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.head.fmt(f)
    }
}

pub struct RequestBuilder {
    method: Option<Method>,
    uri: Option<Uri>,
    headers: Vec<HttpHeader>,
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

    pub fn uri<U: AsRef<[u8]>>(mut self, uri: U) -> Self {
        // TODO: Implement a complete() parser combinator to deal with ensuring nothing
        // is left
        self.uri = match complete(parse_request_target)(Bytes::from(uri.as_ref())) {
            Ok((u, _)) => Some(u.into_uri()),
            Err(e) => {
                self.error = Some(format_err!("Invalid request uri: {:?}", e));
                None
            }
        };

        self
    }

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

        let value = match value.to_header_value(&name) {
            Ok(v) => v,
            Err(e) => {
                self.error = Some(e);
                return self;
            }
        };

        self.headers.push(HttpHeader { name, value });
        self
    }

    pub fn body(mut self, body: Box<dyn Body>) -> Self {
        self.body = Some(body);
        self
    }

    pub fn build(self) -> Result<Request> {
        if let Some(e) = self.error {
            return Err(e);
        }

        let method = self.method.ok_or_else(|| err_msg("No method specified"))?;

        // TODO: Only certain types of uris are allowed here
        let uri = self.uri.ok_or_else(|| err_msg("No uri specified"))?;

        let headers = HttpHeaders::from(self.headers);

        let body = self.body.ok_or_else(|| err_msg("No body specified"))?;

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