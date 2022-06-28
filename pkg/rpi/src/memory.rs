use std::ffi::CStr;
use std::fs::File;
use std::os::unix::prelude::{FromRawFd, AsRawFd};

use common::errors::*;
use sys::MappedMemory;

const MEM_FILE_PATH: &'static [u8] = b"/dev/mem\0";
const GPIOMEM_FILE_PATH: &'static [u8] = b"/dev/gpiomem\0";

pub const GPIO_PERIPHERAL_OFFSET: u32 = 0x00200000;
pub const GPIO_PERIPHERAL_SIZE: usize = 244;

pub const PWM0_PERIPHERAL_OFFSET: u32 = 0x0020c000;
pub const PWM1_PERIPHERAL_OFFSET: u32 = 0x0020c800;

pub struct MemoryBlock {
    memory: MappedMemory,
}

impl MemoryBlock {
    pub fn open(offset: u32, size: usize) -> Result<Self> {
        Self::open_impl(MEM_FILE_PATH, offset, size)
    }

    pub fn open_peripheral(relative_offset: u32, size: usize) -> Result<Self> {
        let file = if relative_offset == GPIO_PERIPHERAL_OFFSET {
            GPIOMEM_FILE_PATH
        } else {
            MEM_FILE_PATH
        };
        let offset = Self::get_peripheral_address()? + relative_offset;
        Self::open_impl(file, offset, size)
    }

    fn open_impl(path: &'static [u8], offset: u32, size: usize) -> Result<Self> {
        // Validate that the path ends in a nullptr.
        let path = CStr::from_bytes_with_nul(path)?;

        let file = unsafe {File::from_raw_fd(
            sys::open(path.as_ptr(), sys::bindings::O_RDWR | sys::bindings::O_SYNC, 0)
            .map_err(|_| err_msg("Failed to open memory device."))?)
        };

        let memory = unsafe {
            MappedMemory::create(
                std::ptr::null_mut(),
                size,
                sys::bindings::PROT_READ | sys::bindings::PROT_WRITE,
                sys::bindings::MAP_SHARED,
                file.as_raw_fd(),
                std::mem::transmute(offset as sys::off_t),
            )
            .map_err(|_| err_msg("Failed to mmap memory block."))?
        };

        // File no longer needed after the mmap
        drop(file);

        Ok(Self { memory })
    }

    pub fn read_register(&self, offset: usize) -> u32 {
        unsafe {
            let addr = std::mem::transmute::<_, usize>(self.memory.addr()) + offset;
            let ptr = std::mem::transmute::<_, *const u32>(addr);
            std::ptr::read_volatile(ptr)
        }
    }

    // TODO: Require &mut
    pub fn modify_register<F: Fn(u32) -> u32>(&self, offset: usize, f: F) {
        unsafe {
            let addr = std::mem::transmute::<_, usize>(self.memory.addr()) + offset;
            let ptr = std::mem::transmute::<_, *mut u32>(addr);
            let mut value = std::ptr::read_volatile(ptr);
            value = f(value);
            std::ptr::write_volatile(ptr, value);
        }
    }

    // TODO: Require &mut
    pub fn write_register(&self, offset: usize, value: u32) {
        unsafe {
            let addr = std::mem::transmute::<_, usize>(self.memory.addr()) + offset;
            let ptr = std::mem::transmute::<_, *mut u32>(addr);
            std::ptr::write_volatile(ptr, value);
        }
    }

    /// Get the base memory offset of all peripherals in memory.
    /// e.g. this will return 0xFE000000 on BCM2711.
    ///
    /// This is based on the bcm_host_get_peripheral_address() C function. See:
    /// https://www.raspberrypi.org/documentation/hardware/raspberrypi/peripheral_addresses.md
    fn get_peripheral_address() -> Result<u32> {
        let ranges = std::fs::read("/proc/device-tree/soc/ranges")?;
        let mut addr = u32::from_be_bytes(*array_ref![&ranges, 4, 4]);
        if addr != 0 {
            return Ok(addr);
        }

        addr = u32::from_be_bytes(*array_ref![&ranges, 8, 4]);
        Ok(addr)
    }
}

unsafe impl Send for MemoryBlock {}
unsafe impl Sync for MemoryBlock {}

