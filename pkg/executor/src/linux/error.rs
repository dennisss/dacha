use std::string::String;

use common::errors::*;
use common::io::{IoError, IoErrorKind};
#[cfg(target_os = "linux")]
use sys::Errno;

#[cfg(target_os = "linux")]
pub trait FromErrno {
    fn from_errno(errno: Errno, message: &str) -> Option<Error>;
}

#[cfg(target_os = "linux")]
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

// TODO: Remove 'Into<Error>' from 'Errno' and force it to be explicitly converted to avoid cases where we miss calling remap_errno.
#[cfg(target_os = "linux")]
pub trait RemapErrno<T> {
    fn remap_errno<E: FromErrno, F: FnOnce() -> String>(self, message: F) -> Result<T>;
}

#[cfg(target_os = "linux")]
impl<T> RemapErrno<T> for Result<T, Errno> {
    fn remap_errno<E: FromErrno, F: FnOnce() -> String>(self, message: F) -> Result<T> {
        self.map_err(|errno| {
            if let Some(e) = E::from_errno(errno, &message()) {
                return e;
            }

            // TODO: Also Include a message in this case.
            errno.into()
        })
    }
}

#[cfg(target_os = "linux")]
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


pub trait FromStdError {
    fn from_std_error(error: std::io::Error, message: &str) -> Result<Error, std::io::Error>;
}

impl FromStdError for IoError {
    fn from_std_error(error: std::io::Error, message: &str) -> Result<Error, std::io::Error> {
        match error.kind() {
            std::io::ErrorKind::ConnectionRefused |
            std::io::ErrorKind::ConnectionReset |
            std::io::ErrorKind::ConnectionAborted |
            std::io::ErrorKind::TimedOut |
            std::io::ErrorKind::NetworkDown |
            std::io::ErrorKind::HostUnreachable |
            std::io::ErrorKind::NetworkUnreachable => Ok(
                IoError::new(IoErrorKind::Aborted, message)
                    .with_source(error.into())
                    .into()
            ),
            std::io::ErrorKind::BrokenPipe => Ok(
                IoError::new(IoErrorKind::RemoteReaderClosed, message)
                    .with_source(error.into())
                    .into()
            ),
            std::io::ErrorKind::UnexpectedEof => Ok(
                IoError::new(IoErrorKind::UnexpectedEof { num_read: 0 }, message)
                    .with_source(error.into())
                    .into()
            ),
            _ => Err(error)
        }
    }
}

// TODO: Remove 'Into<Error>' from 'std::io::Error' and force it to be explicitly converted to avoid cases where we miss calling remap_errno.
pub trait RemapStdError<T> {
    fn remap_std_error<E: FromStdError, F: FnOnce() -> String>(self, message: F) -> Result<T>;
}

impl<T> RemapStdError<T> for Result<T, std::io::Error> {
    fn remap_std_error<E: FromStdError, F: FnOnce() -> String>(self, message: F) -> Result<T> {
        self.map_err(|e| {
            let msg = message();
            match E::from_std_error(e, &msg) {
                Ok(e) => e,
                Err(e) => format_err!("{}: {}", msg, e)
            }
        })
    }
}
