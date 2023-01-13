use common::{errors::*, io::IoError};
use executor::FromErrno;
use sys::Errno;

/// Errors that occur during file operations.
#[error]
#[derive(PartialEq)]
pub enum FileError {
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

impl FromErrno for FileError {
    fn from_errno(errno: Errno) -> Option<Error> {
        if let Some(err) = IoError::from_errno(errno) {
            return Some(err);
        }

        Some(
            match errno {
                Errno::ENOENT => FileError::NotFound,
                Errno::EEXIST => FileError::AlreadyExists,
                Errno::EPERM => FileError::PermissionDenied,
                Errno::EDQUOT => FileError::OutOfQuota,
                Errno::ENOSPC => FileError::OutOfDiskSpace,
                Errno::ENOTDIR => FileError::NotADirectory,
                Errno::ENAMETOOLONG | Errno::EINVAL => FileError::InvalidPath,
                _ => return None,
            }
            .into(),
        )
    }
}
