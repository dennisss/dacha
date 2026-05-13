use std::ffi::CString;

use base_error::*;

use crate::file::OpenFileDescriptor;
use crate::ioctl::*;
use crate::{bindings, open, O_CLOEXEC, O_RDONLY, O_RDWR, iowr, iow, c_int, MappedMemory};

pub struct DMAHeap {
    fd: OpenFileDescriptor,
}

impl DMAHeap {

    /// Opens the default DMA heap which is not guaranteed to be physically contiguous. 
    pub fn open_default() -> Result<Self> {
        Self::open("/dev/dma_heap/system")
    }

    pub fn open_contiguous() -> Result<Self> {
        Self::open("/dev/dma_heap/linux,cma")
    }

    pub fn open(path: &str) -> Result<Self> {
        if !path.starts_with("/dev/dma_heap/") {
            return Err(err_msg("Not a DMA heap device"));
        }

        let path = CString::new(path).unwrap();

        let fd = OpenFileDescriptor::new(unsafe { open(core::mem::transmute(path.as_ptr()), O_RDONLY | O_CLOEXEC, 0) }?);
        Ok(Self { fd })
    }

    pub fn allocate(&mut self, len: usize) -> Result<DMABuffer> {

        let mut alloc_data = bindings::dma_heap_allocation_data::default();
        alloc_data.len = len as u64;
        alloc_data.fd_flags = O_RDWR | O_CLOEXEC;

        unsafe {
            raw::dma_heap_alloc(*self.fd, &mut alloc_data)
        }?;

        let fd = OpenFileDescriptor::new(alloc_data.fd as i32);
        Ok(DMABuffer { fd, size: len })
    }
}

pub struct DMABuffer {
    fd: OpenFileDescriptor,
    size: usize,
}

impl DMABuffer {
    pub fn len(&self) -> usize {
        self.size
    }

    pub fn sync_read_start(&mut self) -> Result<()> {
        let mut sync = bindings::dma_buf_sync::default();
        sync.flags = (bindings::DMA_BUF_SYNC_START | bindings::DMA_BUF_SYNC_READ) as u64;

        unsafe {
            raw::dma_buf_sync(*self.fd, &sync)
        }?;

        Ok(())
    }

    pub fn sync_read_start_partial(&mut self, offset: u64, len: u64) -> Result<()> {
        let mut sync = bindings::dma_buf_sync_partial::default();
        sync.flags = (bindings::DMA_BUF_SYNC_START | bindings::DMA_BUF_SYNC_READ) as u64;
        sync.offset = offset;
        sync.length = len;

        unsafe {
            raw::dma_buf_sync_partial(*self.fd, &sync)
        }?;

        Ok(())
    }

    pub fn sync_read_end(&mut self) -> Result<()> {
        let mut sync = bindings::dma_buf_sync::default();
        sync.flags = (bindings::DMA_BUF_SYNC_END | bindings::DMA_BUF_SYNC_READ) as u64;

        unsafe {
            raw::dma_buf_sync(*self.fd, &sync)
        }?;

        Ok(())
    }

    pub fn raw_fd(&self) -> c_int {
        *self.fd
    }

    pub fn mmap(&self) -> Result<MappedMemory> {
        Ok(unsafe {
            MappedMemory::create(
                std::ptr::null_mut(),
                self.size,
                bindings::PROT_READ | bindings::PROT_WRITE,
                bindings::MAP_SHARED,
                *self.fd,
                0,
            )?
        })
    }
}


mod raw {
    use super::*;

    iowr!(dma_heap_alloc, bindings::DMA_HEAP_IOC_MAGIC, 0x00, bindings::dma_heap_allocation_data);

    iow!(dma_buf_sync, bindings::DMA_BUF_BASE, 0x00, bindings::dma_buf_sync);

    iow!(dma_buf_sync_partial, bindings::DMA_BUF_BASE, 100, bindings::dma_buf_sync_partial);
}

