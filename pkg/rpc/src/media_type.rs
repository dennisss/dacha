use http::headers::content_type::{parse_content_type, MediaType};

const GRPC_MEDIA_TYPE: &'static str = "application";

const GRPC_MEDIA_SUBTYPE: &'static str = "grpc";

const GRPC_WEB_MEDIA_SUBTYPE: &'static str = "grpc-web";

const GRPC_MEDIA_PROTO_SUFFIX: &'static str = "proto";

const GRPC_MEDIA_JSON_SUFFIX: &'static str = "json";

#[derive(Clone)]
pub struct RPCMediaType {
    pub protocol: RPCMediaProtocol,
    pub serialization: RPCMediaSerialization,
}

#[derive(Clone, Copy, PartialEq)]
pub enum RPCMediaProtocol {
    /// Standard gRPC over HTTP2
    Default,

    /// gRPC over HTTPx with web compatibility.
    Web,
}

#[derive(Clone, Copy, PartialEq)]
pub enum RPCMediaSerialization {
    Proto,
    JSON,
}

impl RPCMediaType {
    pub fn parse(headers: &http::header::Headers) -> Option<Self> {
        let media_type = match parse_content_type(headers) {
            Ok(Some(v)) => v,
            _ => {
                return None;
            }
        };

        if media_type.typ != GRPC_MEDIA_TYPE {
            return None;
        }

        let protocol = match media_type.subtype.as_str() {
            GRPC_MEDIA_SUBTYPE => RPCMediaProtocol::Default,
            GRPC_WEB_MEDIA_SUBTYPE => RPCMediaProtocol::Web,
            _ => {
                return None;
            }
        };

        let suffix = match media_type.suffix {
            Some(v) => v,
            None => {
                return None;
            }
        };

        let serialization = match suffix.as_str() {
            GRPC_MEDIA_PROTO_SUFFIX => RPCMediaSerialization::Proto,
            GRPC_MEDIA_JSON_SUFFIX => RPCMediaSerialization::JSON,
            _ => {
                return None;
            }
        };

        Some(Self {
            protocol,
            serialization,
        })
    }

    pub fn to_string(&self) -> String {
        MediaType {
            typ: GRPC_MEDIA_TYPE.to_string(),
            subtype: match self.protocol {
                RPCMediaProtocol::Default => GRPC_MEDIA_SUBTYPE.to_string(),
                RPCMediaProtocol::Web => GRPC_WEB_MEDIA_SUBTYPE.to_string(),
            },
            suffix: Some(match self.serialization {
                RPCMediaSerialization::Proto => GRPC_MEDIA_PROTO_SUFFIX.to_string(),
                RPCMediaSerialization::JSON => GRPC_MEDIA_JSON_SUFFIX.to_string(),
            }),
            params: vec![],
        }
        .to_string()
    }
}
