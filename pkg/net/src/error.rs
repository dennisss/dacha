use common::io::IoError;
use executor::FromErrno;
use sys::Errno;

use common::errors::*;

#[error]
pub enum NetworkError {
    PermissionDenied,

    AddressInUse,

    AddressNotAvailable,
}

impl FromErrno for NetworkError {
    fn from_errno(errno: Errno) -> Option<Error> {
        if let Some(err) = IoError::from_errno(errno) {
            return Some(err);
        }

        Some(
            match errno {
                Errno::EACCES => NetworkError::PermissionDenied,
                Errno::EADDRINUSE => NetworkError::AddressInUse,
                Errno::EADDRNOTAVAIL => NetworkError::AddressNotAvailable,
                _ => return None,
            }
            .into(),
        )
    }
}
