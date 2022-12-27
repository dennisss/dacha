use alloc::boxed::Box;

use common::errors::*;
use common::io::{Readable, Writeable};
use executor::FileHandle;
use sys::OpenFileDescriptor;

// TODO: We should probably implement some locking around these to ensure that
// there is only one user.

pub struct Stdin {
    file: FileHandle,
}

impl Stdin {
    pub fn get() -> Self {
        let mut fd = OpenFileDescriptor::new(0);
        unsafe { fd.leak() };
        Self {
            file: FileHandle::new(fd, false),
        }
    }
}

#[async_trait]
impl Readable for Stdin {
    async fn read(&mut self, output: &mut [u8]) -> Result<usize> {
        self.file.read(output).await
    }
}

pub struct Stdout {
    file: FileHandle,
}

impl Stdout {
    pub fn get() -> Self {
        let mut fd = OpenFileDescriptor::new(1);
        unsafe { fd.leak() };
        Self {
            file: FileHandle::new(fd, false),
        }
    }
}

#[async_trait]
impl Writeable for Stdout {
    async fn write(&mut self, data: &[u8]) -> Result<usize> {
        self.file.write(data).await
    }

    async fn flush(&mut self) -> Result<()> {
        todo!()
    }
}
