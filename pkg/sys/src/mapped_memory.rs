use crate::errno::*;
use crate::{bindings, c_int, c_size_t, c_uint, off_t};

/// Wrapper around a block of memory mapped to a file by mmap().
/// This wrapper handles ensuring that munmap() is eventually called to clean up
/// the memory.
pub struct MappedMemory {
    addr: *mut u8,
    length: c_size_t,
}

// By itself, the memory is shared by all threads. We can't clone the instance
// though as we still require that there is only one owner of the memory that
// can clean it up with a single munmap call.
unsafe impl Send for MappedMemory {}
unsafe impl Sync for MappedMemory {}

impl Drop for MappedMemory {
    fn drop(&mut self) {
        unsafe { raw::munmap(self.addr, self.length).unwrap() };
    }
}

impl MappedMemory {
    /// Creates a new mapping given the arguments to mmap().
    pub unsafe fn create(
        addr: *mut u8,
        length: c_size_t,
        prot: c_uint,
        flags: c_uint,
        fd: c_int,
        offset: off_t,
    ) -> Result<Self, Errno> {
        let addr = raw::mmap(addr, length, prot, flags, fd, offset)?;
        Ok(Self { addr, length })
    }

    pub fn addr(&self) -> *mut u8 {
        self.addr
    }

    pub fn len(&self) -> usize {
        self.length as usize
    }
}

mod raw {
    use super::*;

    syscall!(mmap, bindings::SYS_mmap, addr: *mut u8, length: c_size_t, prot: c_uint, flags: c_uint, fd: c_int, offset: off_t => Result<*mut u8>);
    syscall!(munmap, bindings::SYS_munmap, addr: *mut u8, length: c_size_t => Result<()>);
}
