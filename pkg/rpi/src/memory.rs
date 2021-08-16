use std::ffi::CStr;

use common::errors::*;
use libc::c_void;

const MEM_FILE_PATH: &'static [u8] = b"/dev/mem\0";

/*
    Should I require a lock to write memory?
*/

pub struct MemoryBlock {
    memory: *mut c_void,
    size: usize,
}

impl MemoryBlock {
    pub fn open(offset: u32, size: usize) -> Result<Self> {
        let path = CStr::from_bytes_with_nul(MEM_FILE_PATH).unwrap();
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_SYNC) };
        if fd < 0 {
            return Err(err_msg("Failed to open memory device."));
        }

        let memory = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                std::mem::transmute(offset as libc::off_t),
            )
        };

        // File no longer needed after the mmap
        unsafe { libc::close(fd) };

        if memory == libc::MAP_FAILED {
            return Err(err_msg("Failed to mmap memory block."));
        }

        Ok(Self { memory, size })
    }

    pub fn read_register(&self, offset: usize) -> u32 {
        unsafe {
            let addr = std::mem::transmute::<_, usize>(self.memory) + offset;
            let ptr = std::mem::transmute::<_, *const u32>(addr);
            std::ptr::read_volatile(ptr)
        }
    }

    pub fn modify_register<F: Fn(u32) -> u32>(&self, offset: usize, f: F) {
        unsafe {
            let addr = std::mem::transmute::<_, usize>(self.memory) + offset;
            let ptr = std::mem::transmute::<_, *mut u32>(addr);
            let mut value = std::ptr::read_volatile(ptr);
            value = f(value);
            std::ptr::write_volatile(ptr, value);
        }
    }

    pub fn write_register(&self, offset: usize, value: u32) {
        unsafe {
            let addr = std::mem::transmute::<_, usize>(self.memory) + offset;
            let ptr = std::mem::transmute::<_, *mut u32>(addr);
            std::ptr::write_volatile(ptr, value);
        }
    }
}

impl Drop for MemoryBlock {
    fn drop(&mut self) {
        unsafe { libc::munmap(self.memory, self.size) };
    }
}
