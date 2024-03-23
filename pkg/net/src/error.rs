use std::string::{String, ToString};

use common::io::IoError;
use executor::FromErrno;
use sys::Errno;

use common::errors::*;

#[error]
pub struct NetworkError {
    pub kind: NetworkErrorKind,
    pub message: String,
}

#[derive(PartialEq, Debug)]
pub enum NetworkErrorKind {
    PermissionDenied,

    AddressInUse,

    AddressNotAvailable,
}

impl NetworkError {
    pub fn new(kind: NetworkErrorKind, message: &str) -> Self {
        Self {
            kind,
            message: message.to_string(),
        }
    }
}

impl FromErrno for NetworkError {
    fn from_errno(errno: Errno, message: &str) -> Option<Error> {
        if let Some(err) = IoError::from_errno(errno, message) {
            return Some(err);
        }

        let kind = match errno {
            Errno::EACCES => NetworkErrorKind::PermissionDenied,
            Errno::EADDRINUSE => NetworkErrorKind::AddressInUse,
            Errno::EADDRNOTAVAIL => NetworkErrorKind::AddressNotAvailable,
            _ => return None,
        };

        Some(Self::new(kind, message).into())
    }
}
