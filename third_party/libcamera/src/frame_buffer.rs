use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::Mutex;

use nix::sys::mman::*;

use crate::errors::*;
use crate::ffi;
use crate::frame_buffer_allocator::FrameBufferAllocatorInner;
use crate::stream::Stream;

pub use ffi::{FrameBufferPlane, FrameMetadata, FramePlaneMetadata, FrameStatus};

/// Memory buffer created by the FrameBufferAllocator for storing stream frames.
///
/// Exclusive access to a FrameBuffer instance is required to mutate the
/// internal memory.
pub struct FrameBuffer {
    #[allow(unused)]
    allocator: Arc<FrameBufferAllocatorInner>,

    /// Reference to the stream for which this frame buffer was created.
    ///
    /// This is owned by the camera. This is only safe to store as this should
    /// be contained in the same camera referenced in the allocator.
    pub(crate) stream: &'static Stream,

    pub(crate) raw: *mut ffi::FrameBuffer,

    planes: Vec<FrameBufferPlane>,

    memory: Option<Vec<&'static [u8]>>,
}

unsafe impl Send for FrameBuffer {}
unsafe impl Sync for FrameBuffer {}

impl FrameBuffer {
    pub(crate) fn new(
        allocator: Arc<FrameBufferAllocatorInner>,
        stream: &Stream,
        raw: *mut ffi::FrameBuffer,
    ) -> Self {
        // NOTE: We assume that these are immutable so we can cache them in Rust.
        let planes = ffi::frame_buffer_planes(unsafe { &*raw });

        Self {
            allocator,
            stream: unsafe { stream.as_static() },
            raw,
            planes,
            memory: None,
        }
    }

    pub fn planes(&self) -> &[FrameBufferPlane] {
        &self.planes
    }

    pub fn metadata(&self) -> FrameMetadata {
        ffi::frame_buffer_metadata(unsafe { &*self.raw })
    }

    /// Gets a reference to the complete segment of memory in this buffer.
    ///
    /// Not all of the memory may have actually been used in the most recent
    /// Request. The caller should check the metadata() to determine how much
    /// was used.
    ///
    /// Memory is returned in the order of the planes but multiple contiguous
    /// planes will be merged into one byte slice. So usually, the returned
    /// vector will contain only one element.
    ///
    /// This will return None until the memory is mmap'ed using map_memory().
    pub fn memory<'a>(&'a self) -> Option<&'a [&'a [u8]]> {
        self.memory.as_ref().map(|v| &v[..])
    }

    /// Attempts to retrieve all the occupied memory as one contigous memory
    /// slice.
    ///
    /// This assumes that the last frame was successfully captured.
    pub fn used_memory(&self) -> Option<&[u8]> {
        let mut size = 0;
        let mut incomplete_plane = false;

        for (plane, plane_meta) in self.planes.iter().zip(self.metadata().planes.iter()) {
            if incomplete_plane {
                return None;
            }

            let n = plane_meta.inner.bytesused as usize;
            size += n;
            incomplete_plane = n != (plane.length as usize);
        }

        let memory = match self.memory() {
            Some(v) => v,
            None => return None,
        };

        if memory.len() != 1 {
            return None;
        }

        Some(&memory[0][0..size])
    }

    /// mmap's this frame buffer's data into the current process so that it can
    /// be accessed via Self::memory().
    pub fn map_memory(&mut self) -> Result<()> {
        if self.memory.is_none() {
            self.memory = Some(self.map_all_memory()?);
        }

        Ok(())
    }

    fn map_all_memory(&self) -> Result<Vec<&'static [u8]>> {
        let mut memory_buffers = vec![];

        let mut current_segment: Option<FrameBufferPlane> = None;
        for plane in self.planes() {
            if let Some(p) = &mut current_segment {
                if p.fd == plane.fd && p.offset + p.length == plane.offset {
                    p.length += plane.length;
                    continue;
                }

                memory_buffers.push(unsafe { Self::mmap_plane(p.clone()) }?);
            }

            current_segment = Some(plane.clone());
        }

        if let Some(p) = current_segment {
            memory_buffers.push(unsafe { Self::mmap_plane(p) }?);
        }

        Ok(memory_buffers)
    }

    unsafe fn mmap_plane(plane: FrameBufferPlane) -> Result<&'static [u8]> {
        let mem = mmap(
            None,
            NonZeroUsize::new(plane.length as usize).unwrap(),
            ProtFlags::PROT_READ | ProtFlags::PROT_WRITE,
            MapFlags::MAP_SHARED,
            plane.fd as i32,
            plane.offset as nix::libc::off_t,
        )?;

        Ok(core::slice::from_raw_parts(
            core::mem::transmute(mem),
            plane.length as usize,
        ))
    }
}
