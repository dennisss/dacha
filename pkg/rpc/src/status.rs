// Standard status codes are defined in:
// https://github.com/grpc/grpc/blob/master/doc/statuscodes.md

use std::fmt::Write;

use common::errors::*;
use http::{Header, Headers};
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;
use protobuf::Message;

use crate::constants::{GRPC_STATUS, GRPC_STATUS_DETAILS, GRPC_STATUS_MESSAGE};

pub type StatusResult<T> = std::result::Result<T, Status>;

// TODO: How do we ensure that we don't propagate internal Status protos that
// are generated (e.g. from calling other RPCs)
#[derive(Debug, Fail, Clone)]
pub struct Status {
    code: StatusCode,

    /// NOTE: Will always be encoded over the wire as UTF-8
    message: String,

    details: Vec<google::proto::any::Any>,
}

macro_rules! status_ctor {
    ($name:ident, $code:ident) => {
        pub fn $name<S: Into<String>>(message: S) -> Self {
            Self {
                code: StatusCode::$code,
                message: message.into(),
                details: vec![],
            }
        }
    };
}

impl Status {
    status_ctor!(aborted, Aborted);
    status_ctor!(cancelled, Cancelled);
    status_ctor!(not_found, NotFound);
    status_ctor!(invalid_argument, InvalidArgument);
    status_ctor!(internal, Internal);
    status_ctor!(already_exists, AlreadyExists);
    status_ctor!(failed_precondition, FailedPrecondition);
    status_ctor!(unimplemented, Unimplemented);
    status_ctor!(unknown, Unknown);

    pub fn code(&self) -> StatusCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn from_headers(headers: &Headers) -> Result<Self> {
        let code_header = headers.find_one(GRPC_STATUS)?;
        let code = std::str::from_utf8(code_header.value.as_bytes())?.parse::<usize>()?;

        let mut message = String::new();
        if headers.has(GRPC_STATUS_MESSAGE) {
            // Raw message (ASCII and still percent encoded)
            let raw_message =
                std::str::from_utf8(headers.find_one(GRPC_STATUS_MESSAGE)?.value.as_bytes())?;

            // TODO: Decode according to the restricted form of allowed characters.
            // Noteably the grpc spec says that we should resilient to errors.

            message = raw_message.to_string();
        }

        let mut details = vec![];
        if headers.has(GRPC_STATUS_DETAILS) {
            let encoded_value = headers
                .find_one(GRPC_STATUS_DETAILS)?
                .value
                .to_ascii_str()?;

            let decoded_value = common::base64::decode_config(
                encoded_value.as_bytes(),
                common::base64::STANDARD_NO_PAD,
            )?;

            let proto = google::proto::rpc::Status::parse(&decoded_value)?;
            for detail in proto.details() {
                details.push(detail.clone());
            }
        }

        Ok(Self {
            code: StatusCode::from_value(code)?,
            message,
            details,
        })
    }

    pub fn ok() -> Self {
        Self {
            code: StatusCode::Ok,
            message: String::new(),
            details: vec![],
        }
    }

    pub fn is_ok(&self) -> bool {
        self.code == StatusCode::Ok
    }

    pub fn append_to_headers(&self, headers: &mut Headers) -> Result<()> {
        headers.raw_headers.push(Header {
            name: AsciiString::from(GRPC_STATUS)?,
            value: OpaqueString::from(self.code.to_value().to_string()),
        });

        if !self.message.is_empty() {
            let mut encoded_message = String::new();
            for byte in self.message.as_bytes() {
                if byte.is_ascii() {
                    encoded_message.push(*byte as char);
                } else {
                    write!(&mut encoded_message, "%{:02X}", byte)?;
                }
            }

            headers.raw_headers.push(Header {
                name: AsciiString::from(GRPC_STATUS_MESSAGE)?,
                value: OpaqueString::from(encoded_message),
            });
        }

        if !self.details.is_empty() {
            let mut proto = google::proto::rpc::Status::default();
            for detail in &self.details {
                proto.add_details(detail.clone());
            }

            let value =
                common::base64::encode_config(&proto.serialize()?, common::base64::STANDARD_NO_PAD);

            headers.raw_headers.push(Header {
                name: AsciiString::from(GRPC_STATUS_DETAILS)?,
                value: OpaqueString::from(value),
            })
        }

        Ok(())
    }

    pub fn detail<T: protobuf::Message + Default>(&self) -> Result<Option<T>> {
        for detail in &self.details {
            if let Some(v) = detail.unpack()? {
                return Ok(Some(v));
            }
        }

        Ok(None)
    }

    pub fn with_detail<M: protobuf::Message>(mut self, value: &M) -> Result<Self> {
        let mut any = google::proto::any::Any::default();
        any.pack_from(value)?;
        self.details.push(any);
        Ok(self)
    }

    pub fn to_proto(&self) -> google::proto::rpc::Status {
        let mut out = google::proto::rpc::Status::default();
        out.set_code(self.code().to_value() as i32);
        out.set_message(self.message());
        for any in &self.details {
            out.add_details(any.clone());
        }

        out
    }
}

// TODO: Also implement from HTTP codes:
// https://github.com/grpc/grpc/blob/master/doc/http-grpc-status-mapping.md

// TODO: Implement error translation and pass through in client and server:
// https://github.com/grpc/grpc/blob/master/doc/PROTOCOL-HTTP2.md
impl std::convert::From<http::v2::ProtocolErrorV2> for Status {
    fn from(e: http::v2::ProtocolErrorV2) -> Self {
        Self {
            code: match e.code {
                http::v2::ErrorCode::NO_ERROR
                | http::v2::ErrorCode::PROTOCOL_ERROR
                | http::v2::ErrorCode::INTERNAL_ERROR
                | http::v2::ErrorCode::FLOW_CONTROL_ERROR
                | http::v2::ErrorCode::SETTINGS_TIMEOUT
                | http::v2::ErrorCode::STREAM_CLOSED
                | http::v2::ErrorCode::FRAME_SIZE_ERROR
                | http::v2::ErrorCode::COMPRESSION_ERROR
                | http::v2::ErrorCode::CONNECT_ERROR => StatusCode::Internal,
                http::v2::ErrorCode::REFUSED_STREAM => StatusCode::Unavailable,
                http::v2::ErrorCode::CANCEL => StatusCode::Cancelled,
                http::v2::ErrorCode::ENHANCE_YOUR_CALM => StatusCode::ResourceExhausted,
                http::v2::ErrorCode::INADEQUATE_SECURITY => StatusCode::PermissionDenied,
                http::v2::ErrorCode::HTTP_1_1_REQUIRED | http::v2::ErrorCode::Unknown(_) => {
                    StatusCode::Unknown
                }
            },
            message: format!("[{:?}] {}", e.code, e.message),
            // TODO: Store the original error.
            details: vec![],
        }
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

// TODO: How do we distinguish statuses returned by the RPC framework vs ones
// generated by the application?
enum_def!(StatusCode usize =>
    Ok = 0,
    Cancelled = 1,
    Unknown = 2,
    InvalidArgument = 3,
    DeadlineExceeded = 4,
    NotFound = 5,
    AlreadyExists = 6,
    PermissionDenied = 7,
    ResourceExhausted = 8,
    FailedPrecondition = 9,
    Aborted = 10,
    OutOfRange = 11,
    Unimplemented = 12,
    Internal = 13,
    Unavailable = 14,
    DataLoss = 15,
    Unauthenticated = 16
);
