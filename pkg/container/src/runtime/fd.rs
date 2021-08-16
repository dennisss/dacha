// Utilities for working with file descriptors especially related to pluming
// them into sub-processes.

use std::os::unix::prelude::{FromRawFd, RawFd};

use common::errors::*;
use nix::fcntl::OFlag;
use nix::sys::stat::Mode;

pub const STDIN: RawFd = 0;
pub const STDOUT: RawFd = 1;
pub const STDERR: RawFd = 2;

/// Assumptions make:
/// - Should never add multiple entries with FileReference::Existing items that
///   point to the same file.
/// - The target fd's do not intersect with the set of existing fd's in the
///   entris
#[derive(Default)]
pub struct FileMapping {
    entries: Vec<(RawFd, FileReference)>,
}

impl FileMapping {
    pub fn add(&mut self, target_fd: RawFd, new_file: FileReference) -> &mut Self {
        self.entries.push((target_fd, new_file));
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &(RawFd, FileReference)> {
        self.entries.iter()
    }
}

pub struct FileReference {
    // NOTE: We use in-indirection through an internal struct to allow changing the handle to None
    // before dropping.
    handle: FileReferenceHandle,
}

enum FileReferenceHandle {
    None,
    Existing(RawFd),
    Path(String),
}

impl Drop for FileReference {
    fn drop(&mut self) {
        if let FileReferenceHandle::Existing(fd) = &self.handle {
            let _ = unsafe { libc::close(*fd) };
        }
    }
}

impl FileReference {
    pub fn path(path: &str) -> Self {
        Self {
            handle: FileReferenceHandle::Path(path.to_string()),
        }
    }

    pub fn open(mut self) -> Result<std::fs::File> {
        let file = match &self.handle {
            FileReferenceHandle::None => panic!("Opening empty FileReference"),
            FileReferenceHandle::Existing(fd) => unsafe { std::fs::File::from_raw_fd(*fd) },
            FileReferenceHandle::Path(path) => std::fs::File::open(path)?,
        };

        // Required in order to prevent the drop() handler from double closing it.
        self.handle = FileReferenceHandle::None;

        Ok(file)
    }

    /// NOTE: When using this function, the descriptor may never be closed.
    pub unsafe fn open_raw(&self) -> Result<RawFd> {
        Ok(match &self.handle {
            FileReferenceHandle::None => panic!("Opening empty FileReference"),
            FileReferenceHandle::Existing(fd) => *fd,
            FileReferenceHandle::Path(path) => {
                nix::fcntl::open(path.as_str(), OFlag::O_CLOEXEC, Mode::S_IRUSR)?
            }
        })
    }

    fn existing(fd: RawFd) -> Self {
        Self {
            handle: FileReferenceHandle::Existing(fd),
        }
    }

    pub fn pipe() -> Result<(Self, Self)> {
        nix::unistd::pipe2(OFlag::O_CLOEXEC)
            .map(|(a, b)| (Self::existing(a), Self::existing(b)))
            .map_err(|e| e.into())
    }
}
