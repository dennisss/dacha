use core::ops::Deref;
use std::ffi::{CStr, CString};

use common::errors::*;

use crate::{c_int, close, open, read, Errno, O_CLOEXEC, O_RDONLY};

/// Wrapper around a file descriptor which closes the descriptor on drop().
pub struct OpenFileDescriptor {
    fd: c_int,
    leak: bool,
}

impl OpenFileDescriptor {
    pub fn new(fd: c_int) -> Self {
        Self { fd, leak: false }
    }

    pub unsafe fn leak(&mut self) {
        self.leak = true;
    }
}

impl Deref for OpenFileDescriptor {
    type Target = c_int;

    fn deref(&self) -> &Self::Target {
        &self.fd
    }
}

impl Drop for OpenFileDescriptor {
    fn drop(&mut self) {
        if !self.leak {
            unsafe {
                // TODO: Check result?
                close(self.fd).unwrap();
            }
        }
    }
}

pub struct BlockingFile {
    fd: OpenFileDescriptor,
}

impl BlockingFile {
    pub fn open(path: &str) -> Result<Self> {
        let path = CString::new(path)?;

        let fd = OpenFileDescriptor::new(unsafe { open(path.as_ptr(), O_RDONLY | O_CLOEXEC, 0) }?);
        Ok(Self { fd })
    }

    pub fn read(&mut self, out: &mut [u8]) -> Result<usize, Errno> {
        unsafe { read(*self.fd, out.as_mut_ptr(), out.len()) }
    }
}

pub fn blocking_read_to_string(path: &str) -> Result<String> {
    let mut out = vec![];

    let mut file = BlockingFile::open(path)?;

    const BLOCK_SIZE: usize = 4096;

    loop {
        let start_offset = out.len();
        let end_offset = start_offset + BLOCK_SIZE;
        out.resize(end_offset, 0);

        let n = file.read(&mut out[start_offset..end_offset])?;
        out.truncate(start_offset + n);

        if n == 0 {
            break;
        }
    }

    Ok(String::from_utf8(out)?)
}
