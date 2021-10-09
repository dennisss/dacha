// Standard status codes are defined in:
// https://github.com/grpc/grpc/blob/master/doc/statuscodes.md

use std::fmt::Write;

use common::errors::*;
use http::{Header, Headers};
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

use crate::constants::{GRPC_STATUS, GRPC_STATUS_MESSAGE};

pub type StatusResult<T> = std::result::Result<T, Status>;

#[derive(Debug, Fail, Clone)]
pub struct Status {
    pub code: StatusCode,

    /// NOTE: Will always be encoded over the wire as UTF-8
    pub message: String,
}

impl Status {
    pub fn cancelled<S: Into<String>>(message: S) -> Self {
        Self {
            code: StatusCode::Cancelled,
            message: message.into(),
        }
    }

    pub fn not_found<S: Into<String>>(message: S) -> Self {
        Self {
            code: StatusCode::NotFound,
            message: message.into(),
        }
    }

    pub fn invalid_argument<S: Into<String>>(message: S) -> Self {
        Self {
            code: StatusCode::InvalidArgument,
            message: message.into(),
        }
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

        Ok(Self {
            code: StatusCode::from_value(code)?,
            message,
        })
    }

    pub fn ok() -> Self {
        Self {
            code: StatusCode::Ok,
            message: String::new(),
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
                    write!(&mut encoded_message, "%{:02X}", byte);
                }
            }

            headers.raw_headers.push(Header {
                name: AsciiString::from(GRPC_STATUS_MESSAGE)?,
                value: OpaqueString::from(encoded_message),
            });
        }

        Ok(())
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.code, self.message)
    }
}

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
    DataLoss = 15,
    Unauthenticated = 16
);
