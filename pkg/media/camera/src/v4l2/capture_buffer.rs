use std::sync::Arc;

use base_error::*;

pub struct CaptureDMABuffer {
    shared: Arc<Shared>,
}

struct Shared {
    buf: sys::DMABuffer,
    memory: sys::MappedMemory,
}

impl CaptureDMABuffer {
    pub fn allocate(buffer_size: usize, count: usize) -> Result<Vec<CaptureDMABuffer>> {
        let mut dma_heap = sys::DMAHeap::open_contiguous()?;

        let mut out = vec![];

        for i in 0..count {
            let buf = dma_heap.allocate(buffer_size)?;
            let memory = buf.mmap()?;

            out.push(Self {
                shared: Arc::new(Shared {
                    buf,
                    memory
                })
            });
        }

        Ok(out)
    }

    pub fn read<'a>(&'a self) -> Result<&'a [u8]> {
        // On a Pi 5 / CM5, this takes around 40us to invalidate a 2MB buffer,
        // NOTE: We never call sync_read_end since that also takes the same amount of time
        // and is generally not needed if promise to only read from the CPU (no writes). 
        self.shared.buf.sync_read_start()?;

        Ok(unsafe { self.read_unsafe() })
    }

    pub fn read_partial<'a>(&self, offset: usize, len: usize) -> Result<&'a [u8]> {
        assert!(offset + len <= self.shared.memory.len());

        self.shared.buf.sync_read_start_partial(offset as u64, len as u64)?;

        Ok(unsafe {
            std::slice::from_raw_parts(
                self.shared.memory.addr().add(offset),
                len
            )
        })
    }

    /// Gets a reference to the entire buffer without doing any book keeping to fix memory
    /// caching.
    ///
    /// This is only safe to call if you trust that the CPU cache has been sufficiently
    /// invalidated to access the data without observing old data from previous captures
    /// into this buffer.
    pub unsafe fn read_unsafe<'a>(&'a self) -> &'a [u8] {
        std::slice::from_raw_parts(self.shared.memory.addr(), self.shared.memory.len())
    }

    pub unsafe fn write_unsafe<'a>(&'a self) -> &'a mut [u8] {
        std::slice::from_raw_parts_mut(self.shared.memory.addr(), self.shared.memory.len())
    }

    pub fn equals(&self, other: &Self) -> bool {
        core::ptr::eq::<Shared>(&*self.shared, &*other.shared)
    }

    /// Makes a copy of this buffer that points to the same internal buffer.
    ///
    /// This is mainly for some special usecases where we want to progressively access the
    /// buffer while the device is writing to it.
    ///
    /// Having two copies of this makes it harder to ensure that devices and the CPU
    /// aren't unsafely accessing the data at the same time.
    pub unsafe fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone()
        }
    } 
}


impl v4l2::DMABufferData for CaptureDMABuffer {
    fn as_raw_fd(&self) -> i32 {
        self.shared.buf.raw_fd()
    }

    fn bytes_used(&self) -> usize {
        0
    }

    fn length(&self) -> usize {
        self.shared.buf.len()
    }
}
