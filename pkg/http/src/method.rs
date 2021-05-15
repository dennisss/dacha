

#[derive(Debug, PartialEq)]
pub enum Method {
    GET,
    HEAD,
    POST,
    PUT,
    DELETE,
    CONNECT,
    OPTIONS,
    TRACE,
    PATCH,
    // /// Only used when upgrading to HTTP2
    // PRI,
}

impl Method {
    pub fn as_str(&self) -> &'static str {
        match self {
            Method::GET => "GET",
            Method::HEAD => "HEAD",
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::DELETE => "DELETE",
            Method::CONNECT => "CONNECT",
            Method::OPTIONS => "OPTIONS",
            Method::TRACE => "TRACE",
            Method::PATCH => "PATCH",
        }
    }
}

impl std::convert::TryFrom<&[u8]> for Method {
    type Error = &'static str;
    fn try_from(value: &[u8]) -> std::result::Result<Self, Self::Error> {
        Ok(match value {
            b"GET" => Method::GET,
            b"HEAD" => Method::HEAD,
            b"POST" => Method::POST,
            b"PUT" => Method::PUT,
            b"DELETE" => Method::DELETE,
            b"CONNECT" => Method::CONNECT,
            b"OPTIONS" => Method::OPTIONS,
            b"TRACE" => Method::TRACE,
            b"PATCH" => Method::PATCH,
            _ => {
                return Err("Invalid method");
            }
        })
    }
}