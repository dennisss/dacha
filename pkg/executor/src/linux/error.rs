use common::errors::*;
use common::io::{IoError, IoErrorKind};
use sys::Errno;

pub trait FromErrno {
    fn from_errno(errno: Errno) -> Option<Error>;
}

impl FromErrno for IoError {
    fn from_errno(errno: Errno) -> Option<Error> {
        match errno {
            Errno::EIO | Errno::ECONNABORTED | Errno::ECONNREFUSED | Errno::ECANCELED => Some(
                IoError::new(IoErrorKind::Aborted, "")
                    .with_source(errno.into())
                    .into(),
            ),
            Errno::EPIPE => Some(
                IoError::new(IoErrorKind::RemoteReaderClosed, "")
                    .with_source(errno.into())
                    .into(),
            ),
            _ => None,
        }
    }
}

pub trait RemapErrno<T> {
    fn remap_errno<E: FromErrno>(self) -> Result<T>;
}

impl<T> RemapErrno<T> for Result<T, Errno> {
    fn remap_errno<E: FromErrno>(self) -> Result<T> {
        self.map_err(|errno| {
            if let Some(e) = E::from_errno(errno) {
                return e;
            }

            errno.into()
        })
    }
}

impl<T> RemapErrno<T> for Result<T> {
    fn remap_errno<E: FromErrno>(self) -> Result<T> {
        self.map_err(|e| {
            if let Some(errno) = e.downcast_ref() {
                if let Some(e) = E::from_errno(*errno) {
                    return e;
                }
            }

            e
        })
    }
}
