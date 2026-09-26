use std::io::{Read, Write, Seek};
#[cfg(target_os = "windows")]
use std::os::windows::fs::FileExt;
#[cfg(target_os = "macos")]
use std::os::unix::fs::FileExt;

use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::ffi::CString;

use alloc::string::String;
use common::errors::*;
use common::io::{IoError, IoErrorKind, Readable, Seekable, Writeable};
// pub use executor::SyncRange;
use executor::error::RemapStdError;
use executor::SyncRange;



use crate::local::options::LocalFileOpenOptions;
use crate::local::LocalPath;
use crate::{FileError, FileErrorKind, LocalPathBuf};


pub type Metadata = std::fs::Metadata;

pub struct LocalFile {
    file: std::fs::File,
    path: LocalPathBuf,
    sync_on_flush: bool,
    offset: u64,
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

    /*
    pub(super) sync_on_flush: bool,
    pub(super) sync: bool,
    pub(super) exclusive: bool,
    pub(super) direct: bool,
    pub(super) non_blocking: bool,
    /// Used when creating new files. Some bits may get masked out by 'umask'.
    pub(super) mode: u32,

    */

    fn open_impl(path: &LocalPath, options: &LocalFileOpenOptions) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(options.read)
            .write(options.write)
            .create(options.create)
            .create_new(options.create_new)
            .truncate(options.truncate)
            .append(options.append)
            .open(path)
            .remap_std_error::<FileError, _>(|| format!("open on '{}' failed", path.display()))?;

        // TODO: Need to sync directory if options.sync_on_flush
        
        Ok(Self {
            file,
            path: path.to_owned(),
            sync_on_flush: options.sync_on_flush,
            offset: 0,
        })
    }

    pub async fn metadata(&self) -> Result<Metadata> {
        Ok(self.file.metadata()?)
    }

    pub fn try_lock_exclusive(&self) -> Result<()> {
        let message = || format!("Failed to acquire exclusive lock on {}", self.path.display());

        match self.file.try_lock() {
            Ok(()) => return Ok(()),
            Err(std::fs::TryLockError::WouldBlock) => {
                Err(FileError::new(FileErrorKind::LockContention, &message()).into())
            }
            Err(std::fs::TryLockError::Error(e)) => {
                Err(e).remap_std_error::<FileError, _>(message)
            }
        }
    }

    #[cfg(target_os = "windows")]
    pub async fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize> {
        self.file.seek_read(output, offset)
            .remap_std_error::<FileError, _>(|| "".into())
    }

    #[cfg(target_os = "macos")]
    pub async fn read_at(&self, offset: u64, output: &mut [u8]) -> Result<usize> {
        self.file.read_at(output, offset)
            .remap_std_error::<FileError, _>(|| "".into())
    }

    // TODO: Dedup this.
    pub async fn read_exact_at(&self, mut offset: u64, mut output: &mut [u8]) -> Result<()> {
        let mut num_read = 0;
        while output.len() > 0 {
            match self.read_at(offset, output).await {
                Ok(0) => {
                    return Err(IoError::new(IoErrorKind::UnexpectedEof { num_read }, "").into());
                }
                Ok(n) => {
                    num_read += n;
                    offset += n as u64;
                    output = &mut output[n..];
                }
                Err(error) => {
                    return Err(error);
                }
            }
        }

        Ok(())
    }

    #[cfg(target_os = "windows")]
    pub async fn write_at(&self, offset: u64, data: &[u8]) -> Result<usize> {
        self.file.seek_write(data, offset)
            .remap_std_error::<FileError, _>(|| "".into())
    }

    #[cfg(target_os = "macos")]
    pub async fn write_at(&self, offset: u64, data: &[u8]) -> Result<usize> {
        self.file.write_at(data, offset)
            .remap_std_error::<FileError, _>(|| "".into())
    }

    pub async fn sync(&self, data_sync: bool, range: Option<SyncRange>) -> Result<()> {
        // TODO: Support ranges.

        if data_sync {
            self.sync_data().await
        } else {
            self.sync_all().await
        }
    }

    // /// WARNING: This is NOT retryable.
    // // TODO: We can't allow a sync to be cancelled as we won't be able to record
    // // what happened.
    // pub async fn sync(&self, data_sync: bool, range: Option<SyncRange>) -> Result<()> {
    //     self.file
    //         .sync(data_sync, range)
    //         .await
    //         .remap_errno::<FileError, _>(|| format!("sync on {} failed", self.path.as_str()))
    // }

    pub async fn sync_data(&self) -> Result<()> {
        self.file.sync_data().remap_std_error::<FileError, _>(|| format!("sync_data() on '{}' failed", self.path.display()))
    }

    pub async fn sync_all(&self) -> Result<()> {
        self.file.sync_all().remap_std_error::<FileError, _>(|| format!("sync_all() on '{}' failed", self.path.display()))
    }

    pub async fn set_len(&mut self, new_size: u64) -> Result<()> {
        self.file.set_len(new_size).remap_std_error::<FileError, _>(|| "".into())
    }

    pub fn seek(&mut self, offset: u64) {
        self.offset = offset;
    }

    // TODO: Only require '&self'
    pub fn current_position(&self) -> u64 {
        self.offset
    }

    // pub async fn set_permissions(&mut self, perms: Permissions) -> Result<()> {
    //     unsafe {
    //         sys::fchmod(self.as_raw_fd(), perms.mode)
    //             .remap_errno::<FileError, _>(|| String::new())?
    //     }
    //     Ok(())
    // }

    pub fn path(&self) -> &LocalPath {
        &self.path
    }
}

#[async_trait]
impl Readable for LocalFile {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        let n = self.read_at(self.offset, output).await?;
        self.offset += n as u64;
        Ok(n)
    }
}

/*
#[async_trait]
impl Seekable for LocalFile {
    async fn seek(&mut self, offset: u64) -> Result<()> {
        self.seek_impl(offset);
        Ok(())
    }
}


*/

#[async_trait]
impl Writeable for LocalFile {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        let n = self.write_at(self.offset, data).await?;
        self.offset += n as u64;
        Ok(n)
    }

    async fn flush(&mut self) -> Result<()> {
        // TODO: Add some cancellation poisoning to this.

        // TODO: Call inner.

        if self.sync_on_flush {
            self.sync(true, None).await?;
        }

        Ok(())
    }
}
