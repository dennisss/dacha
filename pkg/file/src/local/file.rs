use std::os::unix::prelude::{AsRawFd, IntoRawFd};

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::ffi::CString;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::{FileHandle, FromErrno, RemapErrno};
use sys::{Errno, OpenFileDescriptor};

use crate::local::path::LocalPath;
use crate::{FileError, LocalPathBuf, Metadata, Permissions};

pub struct LocalFileOpenOptions {
    read: bool,
    write: bool,
    create: bool,
    create_new: bool,
    truncate: bool,
    append: bool,

    /// Used when creating new files. Some bits may get masked out by 'umask'.
    mode: u32,
}

impl LocalFileOpenOptions {
    pub fn new() -> Self {
        Self {
            read: true,
            write: false,
            create: false,
            create_new: false,
            truncate: false,
            append: false,
            mode: 0o666,
        }
    }

    pub fn read(&mut self, value: bool) -> &mut Self {
        self.read = value;
        self
    }

    pub fn write(&mut self, value: bool) -> &mut Self {
        self.write = value;
        self
    }

    pub fn create(&mut self, value: bool) -> &mut Self {
        self.create = value;
        self
    }

    pub fn create_new(&mut self, value: bool) -> &mut Self {
        self.create_new = value;
        self
    }

    pub fn truncate(&mut self, value: bool) -> &mut Self {
        self.truncate = value;
        self
    }

    pub fn append(&mut self, value: bool) -> &mut Self {
        self.append = value;
        self
    }
}

pub struct LocalFile {
    file: FileHandle,
    path: LocalPathBuf,
}

impl LocalFile {
    pub fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        Self::open_impl(path.as_ref(), &LocalFileOpenOptions::new())
    }

    pub fn open_with_options<P: AsRef<LocalPath>>(
        path: P,
        options: &LocalFileOpenOptions,
    ) -> Result<Self> {
        Self::open_impl(path.as_ref(), &options)
    }

    fn open_impl(path: &LocalPath, options: &LocalFileOpenOptions) -> Result<Self> {
        // TODO: Use an async open.

        let cpath = CString::new(path.as_str())?;

        let mut flags = sys::O_RDONLY | sys::O_CLOEXEC;
        if options.create || options.create_new {
            flags |= sys::O_CREAT;
        }
        if options.create_new {
            flags |= sys::O_EXCL;
        }
        if options.write || options.append {
            flags |= sys::O_RDWR;
        }
        if options.truncate {
            flags |= sys::O_TRUNC;
        }
        if options.append {
            flags |= sys::O_APPEND;
        }

        let fd = sys::OpenFileDescriptor::new(
            unsafe { sys::open(cpath.as_ptr(), flags, options.mode as u16) }
                .remap_errno::<FileError>()?,
        );

        Ok(Self {
            file: FileHandle::new(fd, true),
            path: path.to_owned(),
        })
    }

    pub unsafe fn as_raw_fd(&self) -> i32 {
        **self.file.as_raw_fd()
    }

    pub async fn metadata(&self) -> Result<Metadata> {
        let mut stat = sys::bindings::stat::default();
        unsafe { sys::fstat(self.as_raw_fd(), &mut stat) }.remap_errno::<FileError>()?;
        Ok(Metadata { inner: stat })
    }

    /// Will return Err(FileError::LockContention) if the file is already
    /// locked.
    pub fn try_lock_exclusive(&self) -> Result<()> {
        if let Err(e) = unsafe { sys::flock(self.as_raw_fd(), sys::LockOperation::LOCK_EX, true) } {
            if e == Errno::EAGAIN {
                return Err(FileError::LockContention.into());
            }

            return Err(FileError::from_errno(e).unwrap_or_else(|| e.into()));
        }

        Ok(())
    }

    // TODO: Use an io_uring
    pub async fn sync_all(&self) -> Result<()> {
        unsafe { sys::fsync(self.as_raw_fd()).remap_errno::<FileError>()? }
        Ok(())
    }

    // TODO: Use an io_uring
    pub async fn sync_data(&self) -> Result<()> {
        unsafe { sys::fdatasync(self.as_raw_fd()).remap_errno::<FileError>()? }
        Ok(())
    }

    pub async fn set_len(&mut self, new_size: u64) -> Result<()> {
        unsafe { sys::ftruncate(self.as_raw_fd(), new_size).remap_errno::<FileError>()? }
        Ok(())
    }

    pub fn seek(&mut self, offset: u64) {
        self.file.seek(offset)
    }

    pub fn current_position(&self) -> u64 {
        self.file.current_position()
    }

    pub async fn set_permissions(&mut self, perms: Permissions) -> Result<()> {
        unsafe { sys::fchmod(self.as_raw_fd(), perms.mode).remap_errno::<FileError>()? }
        Ok(())
    }
}

impl std::convert::From<std::fs::File> for LocalFile {
    fn from(f: std::fs::File) -> Self {
        // TODO: Possibly not seekable?
        LocalFile {
            file: FileHandle::new(OpenFileDescriptor::new(f.into_raw_fd()), true),
            path: LocalPathBuf::from("/nonexistent"),
        }
    }
}

#[async_trait]
impl Readable for LocalFile {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.file.read(output).await
    }
}

#[async_trait]
impl Writeable for LocalFile {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.file.write(data).await
    }

    async fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}
