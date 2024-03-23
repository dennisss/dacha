use std::string::String;

use common::errors::*;
use common::io::{IoError, IoErrorKind};
use sys::Errno;

pub trait FromErrno {
    fn from_errno(errno: Errno, message: &str) -> Option<Error>;
}

impl FromErrno for IoError {
    fn from_errno(errno: Errno, message: &str) -> Option<Error> {
        match errno {
            Errno::EIO
            | Errno::ECONNRESET
            | Errno::ECONNABORTED
            | Errno::ECONNREFUSED
            | Errno::ECANCELED => Some(
                IoError::new(IoErrorKind::Aborted, message)
                    .with_source(errno.into())
                    .into(),
            ),
            Errno::EPIPE => Some(
                IoError::new(IoErrorKind::RemoteReaderClosed, message)
                    .with_source(errno.into())
                    .into(),
            ),
            _ => None,
        }
    }
}

pub trait RemapErrno<T> {
    fn remap_errno<E: FromErrno, F: FnOnce() -> String>(self, message: F) -> Result<T>;
}

impl<T> RemapErrno<T> for Result<T, Errno> {
    fn remap_errno<E: FromErrno, F: FnOnce() -> String>(self, message: F) -> Result<T> {
        self.map_err(|errno| {
            if let Some(e) = E::from_errno(errno, &message()) {
                return e;
            }

            errno.into()
        })
    }
}

impl<T> RemapErrno<T> for Result<T> {
    fn remap_errno<E: FromErrno, F: FnOnce() -> String>(self, message: F) -> Result<T> {
        self.map_err(|e| {
            if let Some(errno) = e.downcast_ref() {
                if let Some(e) = E::from_errno(*errno, &message()) {
                    return e;
                }
            }

            e
        })
    }
}
