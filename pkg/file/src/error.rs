use std::string::String;

use alloc::string::ToString;
use common::{errors::*, io::IoError};
use executor::error::*;
#[cfg(target_os = "linux")]
use sys::Errno;

/// Errors that occur during file operations.
#[error]
pub struct FileError {
    pub kind: FileErrorKind,
    pub message: String,
}

#[derive(PartialEq, Debug, Clone, Copy)]
pub enum FileErrorKind {
    /// While trying to find a file, the location in which we are searching is
    /// valid but we could not find the file.
    NotFound,

    /// Normally indicates that we tried opening a file whose path contains a
    /// non-directory parent.
    NotADirectory,

    InvalidPath,

    /// We attempted to access a file which we don't have access to read/write.
    PermissionDenied,

    /// We attempted to create a file with create_new(true) but it already
    /// exists.
    AlreadyExists,

    /// We failed to lock a file because it is already locked by another
    /// process.
    LockContention,

    OutOfQuota,

    OutOfDiskSpace,
}

impl FileError {
    pub fn new(kind: FileErrorKind, message: &str) -> Self {
        Self {
            kind,
            message: message.to_string(),
        }
    }
}

#[cfg(target_os = "linux")]
impl FromErrno for FileError {
    fn from_errno(errno: Errno, message: &str) -> Option<Error> {
        if let Some(err) = IoError::from_errno(errno, message) {
            return Some(err);
        }

        let kind = match errno {
            Errno::ENOENT => FileErrorKind::NotFound,
            Errno::EEXIST => FileErrorKind::AlreadyExists,
            Errno::EPERM => FileErrorKind::PermissionDenied,
            Errno::EDQUOT => FileErrorKind::OutOfQuota,
            Errno::ENOSPC => FileErrorKind::OutOfDiskSpace,
            Errno::ENOTDIR => FileErrorKind::NotADirectory,
            Errno::ENAMETOOLONG | Errno::EINVAL => FileErrorKind::InvalidPath,
            _ => return None,
        };

        Some(FileError::new(kind, message).into())
    }
}

impl FromStdError for FileError {
    fn from_std_error(error: std::io::Error, message: &str) -> Result<Error, std::io::Error> {
        let error = match IoError::from_std_error(error, message) {
            Ok(v) => return Ok(v),
            Err(e) => e
        };

        let kind = match error.kind() {
            std::io::ErrorKind::NotFound => FileErrorKind::NotFound,
            std::io::ErrorKind::AlreadyExists => FileErrorKind::AlreadyExists,
            std::io::ErrorKind::NotADirectory => FileErrorKind::NotADirectory,
            std::io::ErrorKind::QuotaExceeded => FileErrorKind::OutOfQuota,
            std::io::ErrorKind::StorageFull => FileErrorKind::OutOfDiskSpace,
            std::io::ErrorKind::InvalidFilename => FileErrorKind::InvalidPath,
            _ => return Err(error)
        };

        Ok(Self::new(kind, message).into())
    }
}