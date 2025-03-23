use std::ffi::CString;

use common::errors::*;
use executor::FileHandle;
use sys::fanotify::*;
use sys::OpenFileDescriptor;
use sys::O_RDONLY;

use crate::LocalPath;

pub struct LocalFileWatcher {
    handle: FileHandle,
}

impl LocalFileWatcher {
    pub fn create() -> Result<Self> {
        let fd = OpenFileDescriptor::new(unsafe {
            sys::inotify::raw::inotify_init1(sys::inotify::IN_CLOEXEC as u32)?
        });

        let handle = FileHandle::new(fd, false);

        Ok(Self { handle })
    }

    pub fn mark(&mut self, path: &LocalPath) -> Result<()> {
        let cpath = CString::new(path.as_str())?;

        unsafe {
            sys::inotify::raw::inotify_add_watch(
                **self.handle.as_raw_fd(),
                cpath.as_ptr(),
                sys::inotify::IN_MODIFY,
            )
            .map_err(|e| format_err!("While calling inotify_add_watch: {}", e))?
        };

        Ok(())
    }

    pub async fn wait(&mut self) -> Result<()> {
        let mut data = vec![0u8; 512];
        self.handle.read(&mut data).await?;

        Ok(())
    }
}

/// Watches one or more files for changes.
/// Internally uses the Linux fanotify API.
///
/// TODO: We currently don't use this since fanotify_mark fails for files on
/// btrfs subvolumes (which is most btrfs mounts).
struct LocalFileWatcherFanotify {
    handle: FileHandle,
}

impl LocalFileWatcherFanotify {
    pub fn create() -> Result<Self> {
        let fd = OpenFileDescriptor::new(unsafe {
            // NOTE: Setting one of the FID mode flags is required to avoid EPERM if running
            // as an unpriveleged user.
            raw::fanotify_init(
                FAN_CLASS_NOTIF | FAN_REPORT_FID | FAN_CLOEXEC,
                O_CLOEXEC | O_RDONLY,
            )?
        });

        let handle = FileHandle::new(fd, false);

        Ok(Self { handle })
    }

    pub fn mark(&mut self, path: &LocalPath) -> Result<()> {
        if !path.starts_with("/") {
            // Must be absolute if we don't specify a dirfd.
            return Err(err_msg("Expected an absolute path to watch"));
        }

        let cpath = CString::new(path.as_str())?;

        unsafe {
            raw::fanotify_mark(
                **self.handle.as_raw_fd(),
                FAN_MARK_ADD,
                FAN_MODIFY as u64,
                0,
                cpath.as_ptr(),
            )
            .map_err(|e| format_err!("While calling fanotify_mark: {}", e))?
        };

        Ok(())
    }

    pub async fn wait(&mut self) -> Result<()> {
        let mut data = [0u8; 512];
        self.handle.read(&mut data).await?;

        Ok(())
    }
}
