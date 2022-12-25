use alloc::boxed::Box;
use alloc::ffi::CString;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::FileHandle;

use crate::local::path::LocalPath;

pub struct LocalFile {
    file: FileHandle,
}

#[derive(Default)]
pub struct LocalFileOpenOptions {
    pub writeable: bool,
    pub create_if_missing: bool,
}

impl LocalFile {
    pub async fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        Self::open_impl(path.as_ref())
    }

    fn open_impl(path: &LocalPath) -> Result<Self> {
        // TODO: Use an async open.

        let path = CString::new(path.as_str())?;

        let fd = sys::OpenFileDescriptor::new(unsafe {
            sys::open(
                path.as_ptr(),
                sys::O_RDONLY | sys::O_CLOEXEC | sys::O_NONBLOCK,
                0, // TODO: Set this.
            )
        }?);

        Ok(Self {
            file: FileHandle::new(fd, true),
        })
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
        todo!()
    }
}
