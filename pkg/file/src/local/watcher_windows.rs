use std::ffi::CString;

use common::errors::*;

use crate::LocalPath;

pub struct LocalFileWatcher {
    // handle: FileHandle,
}

impl LocalFileWatcher {
    pub fn create() -> Result<Self> {
        todo!()

        // let fd = OpenFileDescriptor::new(unsafe {
        //     sys::inotify::raw::inotify_init1(sys::inotify::IN_CLOEXEC as u32)?
        // });

        // let handle = FileHandle::new(fd, false);

        // Ok(Self { handle })
    }

    pub fn mark(&mut self, path: &LocalPath) -> Result<()> {
        // let cpath = CString::new(path.as_str())?;

        // unsafe {
        //     sys::inotify::raw::inotify_add_watch(
        //         **self.handle.as_raw_fd(),
        //         cpath.as_ptr(),
        //         sys::inotify::IN_MODIFY,
        //     )
        //     .map_err(|e| format_err!("While calling inotify_add_watch: {}", e))?
        // };

        todo!();

        Ok(())
    }

    pub async fn wait(&mut self) -> Result<()> {
        // let mut data = vec![0u8; 512];
        // self.handle.read(&mut data).await?;

        todo!();

        Ok(())
    }
}
