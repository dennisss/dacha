use std::os::unix::prelude::{AsRawFd, IntoRawFd};

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::ffi::CString;

use common::errors::*;
use common::io::{Readable, Seekable, Writeable};
use executor::{FileHandle, FromErrno, RemapErrno};
use sys::{Errno, OpenFileDescriptor};

use crate::local::path::LocalPath;
use crate::{FileError, LocalPathBuf, Metadata, Permissions};

pub struct LocalFileOpenOptions {
    read: bool,
    write: bool,
    create: bool,
    create_new: bool,
    sync_on_flush: bool,
    truncate: bool,
    append: bool,
    sync: bool,
    exclusive: bool,

    direct: bool,

    non_blocking: bool,

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
            sync_on_flush: false,
            truncate: false,
            append: false,
            direct: false,
            sync: false,
            exclusive: false,
            non_blocking: false,
            mode: 0o666,
        }
    }

    pub fn read(&mut self, value: bool) -> &mut Self {
        self.read = value;
        self
    }

    pub fn direct(&mut self, value: bool) -> &mut Self {
        self.direct = value;
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

    pub fn sync(&mut self, value: bool) -> &mut Self {
        self.sync = value;
        self
    }

    pub fn exclusive(&mut self, value: bool) -> &mut Self {
        self.exclusive = value;
        self
    }

    pub fn non_blocking(&mut self, value: bool) -> &mut Self {
        self.non_blocking = value;
        self
    }

    /// Normally when flush() is called, it will unblock when all written data
    /// has been transferred out of the current process. But if this is set to
    /// true, it will also wait for the data to be durably written to disk (or
    /// whatever the final destination is for the filesystem).
    pub fn sync_on_flush(&mut self, value: bool) -> &mut Self {
        self.sync_on_flush = value;
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
    sync_on_flush: bool,
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

        // TODO: When implementing openat,

        if path.as_str().is_empty() {
            return Err(FileError::NotFound.into());
        }

        // Make absolute so that it's easier to find the directory.
        let path = {
            if path.is_absolute() {
                path.to_owned()
            } else {
                crate::current_dir()?.join(path)
            }
            .normalized()
        };

        let mut flags = sys::O_RDONLY | sys::O_CLOEXEC;
        if options.create || options.create_new {
            flags |= sys::O_CREAT;
        }
        if options.create_new {
            flags |= sys::O_EXCL;
        }
        if (options.write || options.append) && !options.read {
            flags |= sys::O_WRONLY;
        } else if options.write || options.append {
            flags |= sys::O_RDWR;
        }
        if options.truncate {
            flags |= sys::O_TRUNC;
        }
        if options.append {
            // TODO: Should we disallow seeking in these types of files.
            flags |= sys::O_APPEND;
        }
        if options.direct {
            flags |= sys::O_DIRECT;
        }
        if options.sync {
            flags |= sys::O_SYNC;
        }
        if options.exclusive {
            // NOTE: Only applicable if not using create_new
            flags |= sys::O_EXCL;
        }
        if options.non_blocking {
            flags |= sys::O_NONBLOCK;
        }

        // TODO: We should also use this approach with mkdirat when creating files.
        let fd = {
            if let Some(dir_path) = path.parent() && options.sync_on_flush {
                let dir_cpath = CString::new(dir_path.as_str())?;

                let dir_fd = sys::OpenFileDescriptor::new(
                    unsafe { sys::open(dir_cpath.as_ptr(), sys::O_DIRECTORY, 0) }
                        .remap_errno::<FileError>()?,
                );

                // TODO: Make these to InvalidPath errors.
                let file_cpath = CString::new(path.file_name().unwrap())?;

                let fd = unsafe {
                    sys::openat(
                        dir_fd.as_raw_fd(),
                        file_cpath.as_ptr(),
                        flags,
                        options.mode as u16,
                    )
                }
                .remap_errno::<FileError>()?;

                // Here we assume that if the sync fails, the file system will revert back to
                // the previous state before the file was created. So if this fails, we won't
                // expose a potentially adandoned file to the caller and they will need to retry
                // against the FS to get the file again (readers should not see it on failures
                // if our assumptions are correct).
                //
                // TODO: For this to be correct, we will also need to fsync while using mkdirat.
                if options.sync_on_flush {
                    unsafe { sys::fsync(dir_fd.as_raw_fd()) }?;
                }

                fd
            } else {
                // This should only happen for opening "/"
                let cpath = CString::new(path.as_str())?;
                unsafe { sys::open(cpath.as_ptr(), flags, options.mode as u16) }
                    .remap_errno::<FileError>()?
            }
        };

        let fd = sys::OpenFileDescriptor::new(fd);

        Ok(Self {
            file: FileHandle::new(fd, true),
            path: path.to_owned(),
            sync_on_flush: options.sync_on_flush,
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
        self.seek_impl(offset)
    }

    fn seek_impl(&mut self, offset: u64) {
        self.file.seek(offset)
    }

    pub fn current_position(&self) -> u64 {
        self.file.current_position()
    }

    pub async fn set_permissions(&mut self, perms: Permissions) -> Result<()> {
        unsafe { sys::fchmod(self.as_raw_fd(), perms.mode).remap_errno::<FileError>()? }
        Ok(())
    }

    pub fn path(&self) -> &LocalPath {
        &self.path
    }
}

impl std::convert::From<std::fs::File> for LocalFile {
    fn from(f: std::fs::File) -> Self {
        // TODO: Possibly not seekable?
        LocalFile {
            file: FileHandle::new(OpenFileDescriptor::new(f.into_raw_fd()), true),
            path: LocalPathBuf::from("/nonexistent"),
            sync_on_flush: false,
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
impl Seekable for LocalFile {
    async fn seek(&mut self, offset: u64) -> Result<()> {
        self.seek_impl(offset);
        Ok(())
    }
}

#[async_trait]
impl Writeable for LocalFile {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.file.write(data).await
    }

    async fn flush(&mut self) -> Result<()> {
        if self.sync_on_flush {
            self.sync_data().await?;
        }

        Ok(())
    }
}
