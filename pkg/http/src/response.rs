use common::errors::*;
use crate::body::*;
use crate::header::*;
use crate::message::*;
use crate::status_code::*;

pub struct Response {
    pub head: ResponseHead,
    pub body: Box<dyn Body>,
}

#[derive(Debug)]
pub struct ResponseHead {
    pub version: HttpVersion,
    pub status_code: StatusCode,
    pub reason: String,
    pub headers: HttpHeaders,
}

impl ResponseHead {
    pub fn serialize(&self, out: &mut Vec<u8>) {
        let status_line = format!(
            "HTTP/{} {} {}\r\n",
            self.version.to_string(),
            self.status_code.as_u16(),
            self.reason.to_string()
        );
        out.extend_from_slice(status_line.as_bytes());

        self.headers.serialize(out);
        out.extend_from_slice(b"\r\n");
    }
}

/// Helper for building a Response object.
pub struct ResponseBuilder {
    status_code: Option<StatusCode>,
    reason: Option<String>,
    headers: Vec<HttpHeader>,
    body: Option<Box<dyn Body>>,

    // First error that occured in the building process
    error: Option<Error>,
}

impl ResponseBuilder {
    pub fn new() -> ResponseBuilder {
        ResponseBuilder {
            status_code: None,
            reason: None,
            headers: vec![],
            body: None,
            error: None,
        }
    }

    pub fn status(mut self, code: StatusCode) -> Self {
        self.status_code = Some(code);
        self
    }

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

    pub fn build(self) -> Result<Response> {
        if let Some(e) = self.error {
            return Err(e);
        }

        let status_code = self
            .status_code
            .ok_or_else(|| err_msg("No status specified"))?;

        // TODO: Support custom reason and don't unwrap this.
        let reason = String::from(status_code.default_reason().unwrap());

        let headers = HttpHeaders::from(self.headers);

        let body = self.body.ok_or_else(|| err_msg("No body specified"))?;

        Ok(Response {
            head: ResponseHead {
                status_code,
                reason,
                version: HTTP_V1_1, // TODO: Always respond with version <= client version?
                headers,
            },
            body,
        })
    }
}
