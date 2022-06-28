use crate::errno::*;
use crate::{mmap, munmap, c_size_t, c_int, c_uint, off_t};

/// Wrapper around a block of memory mapped to a file by mmap().
/// This wrapper handles ensuring that munmap() is eventually called to clean up the memory.
pub struct MappedMemory {
    addr: *mut u8,
    length: c_size_t
}

impl Drop for MappedMemory {
    fn drop(&mut self) {
        unsafe { munmap(self.addr, self.length).unwrap() };
    }
}

impl MappedMemory {
    /// Creates a new mapping given the arguments to mmap().
    pub unsafe fn create(
        addr: *mut u8, length: c_size_t, prot: c_uint, flags: c_uint, fd: c_int, offset: off_t
    ) -> Result<Self, Errno> {
        let addr = mmap(addr, length, prot, flags, fd, offset)?;
        Ok(Self { addr, length })
    }

    pub fn addr(&self) -> *mut u8 {
        self.addr
    }

    pub fn len(&self) -> usize {
        self.length as usize
    } 
}
