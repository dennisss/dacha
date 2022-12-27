use std::time::SystemTime;

use crate::FileType;

/*
See https://man7.org/linux/man-pages/man7/inode.7.html
*/

pub struct Metadata {
    pub(crate) inner: sys::bindings::stat,
}

impl Metadata {
    pub fn len(&self) -> u64 {
        self.inner.st_size as u64
    }

    pub fn gid(&self) -> u32 {
        self.inner.st_gid
    }

    pub fn modified(&self) -> SystemTime {
        todo!()
    }

    pub fn permissions(&self) -> Permissions {
        Permissions {
            mode: self.inner.st_mode & 0o7777,
        }
    }

    /*
    pub fn file_type(&self) -> FileType {
        self.inner.st
    }
     */

    /// NOTE: May  be smaller than 'len' for files with holes.
    pub fn allocated_size(&self) -> u64 {
        (self.inner.st_blocks as u64) * 512
    }

    pub fn is_file(&self) -> bool {
        (self.inner.st_mode & sys::bindings::S_IFMT) == sys::bindings::S_IFREG
    }

    pub fn is_dir(&self) -> bool {
        (self.inner.st_mode & sys::bindings::S_IFMT) == sys::bindings::S_IFDIR
    }

    pub fn is_symlink(&self) -> bool {
        (self.inner.st_mode & sys::bindings::S_IFMT) == sys::bindings::S_IFLNK
    }

    pub fn st_uid(&self) -> u32 {
        self.inner.st_uid
    }

    pub fn st_gid(&self) -> u32 {
        self.inner.st_gid
    }

    pub fn st_mode(&self) -> u32 {
        self.inner.st_mode
    }
}

/// Includes file mode (set-user-id, set-group-id, sticky bits) and the
/// permissions.
#[derive(Clone, Copy)]
pub struct Permissions {
    pub(crate) mode: u32,
}

impl Permissions {
    pub fn mode(&self) -> u32 {
        self.mode
    }

    pub fn set_mode(&mut self, value: u32) {
        self.mode = value;
    }
}
