use std::string::{String, ToString};

use common::io::IoError;
use executor::error::*;
#[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
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

impl FromStdError for NetworkError {
    fn from_std_error(error: std::io::Error, message: &str) -> Result<Error, std::io::Error> {
        let error = match IoError::from_std_error(error, message) {
            Ok(v) => return Ok(v),
            Err(e) => e
        };

        let kind = match error.kind() {
            std::io::ErrorKind::PermissionDenied => NetworkErrorKind::PermissionDenied,
            std::io::ErrorKind::AddrInUse => NetworkErrorKind::AddressInUse,
            std::io::ErrorKind::AddrNotAvailable => NetworkErrorKind::AddressNotAvailable,
            _ => return Err(error)
        };

        Ok(Self::new(kind, message).into())
    }
}
