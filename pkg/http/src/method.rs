

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
}

impl Method {
    pub fn as_str(&self) -> &'static [u8] {
        match self {
            Method::GET => b"GET",
            Method::HEAD => b"HEAD",
            Method::POST => b"POST",
            Method::PUT => b"PUT",
            Method::DELETE => b"DELETE",
            Method::CONNECT => b"CONNECT",
            Method::OPTIONS => b"OPTIONS",
            Method::TRACE => b"TRACE",
            Method::PATCH => b"PATCH",
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