use core::time::Duration;
use std::time::SystemTime;

use crate::FileType;
use crate::local::device_num::DeviceNumber;

/*
See https://man7.org/linux/man-pages/man7/inode.7.html
*/

#[derive(Debug)]
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
        let t = &self.inner;
        assert!(t.st_mtime >= 0 && t.st_mtime_nsec >= 0);

        SystemTime::UNIX_EPOCH
            + Duration::from_secs(t.st_mtime as u64)
            + Duration::from_nanos(t.st_mtime_nsec as u64)
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

    pub fn is_block_dev(&self) -> bool {
        (self.inner.st_mode & sys::bindings::S_IFMT) == sys::bindings::S_IFBLK
    }

    pub fn is_character_dev(&self) -> bool {
        (self.inner.st_mode & sys::bindings::S_IFMT) == sys::bindings::S_IFCHR
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

    /// This is the device of the filesystem on which this file is located.
    pub fn st_dev(&self) -> sys::dev_t {
        self.inner.st_dev
    }

    /// This is the device that this file represents.
    ///
    /// NOTE: Only applicable for block and character devices.
    pub fn represented_device(&self) -> DeviceNumber {
        DeviceNumber::from_raw(self.inner.st_rdev)
    }
}

/// Includes file mode (set-user-id, set-group-id, sticky bits) and the
/// permissions.
#[derive(Clone, Copy, Default)]
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
