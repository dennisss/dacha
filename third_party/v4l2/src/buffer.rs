use std::time::Duration;
use std::{collections::HashSet, sync::Arc};

use base_error::*;
use file::{LocalFile, LocalFileOpenOptions, LocalPath};
use sys::MappedMemory;

use crate::bindings::*;
use crate::io::*;
use crate::DeviceHandle;

/// Wrapper around a v4l2_buffer struct containing the current state of a buffer
/// which is currently owned by userspace.
///
/// Internally this assumes that the buffer is either single-planar or is
/// multi-plane with just one plane being used.
pub struct RawBuffer {
    /// Maintain a reference to the device to ensure that the buffer outlives
    /// the
    pub(crate) device: Arc<DeviceHandle>,

    pub(crate) buffer: v4l2_buffer,

    /// Plane array in buffer.m.planes used when using a multi-plane buffer.
    ///
    /// NOTE: This must never change in size to ensure that the memory pointers
    /// to the planes don't change.
    /// TODO: Enforce the above with the compiler.
    pub(crate) planes: Vec<v4l2_plane>,
}

// TODO: Document why this is ok.
unsafe impl Send for RawBuffer {}
unsafe impl Sync for RawBuffer {}

pub trait Buffer {
    /// Userspace data/resources that must be locked in order to access this
    /// buffer.
    type Data;

    /// This is unsafe because the caller must guarantee that the correct 'data'
    /// that is associated with 'raw' was provided.
    unsafe fn from_raw_parts(raw: RawBuffer, data: Self::Data) -> Self;

    /// Unsafe as the lifetime of 'Data' will no longer be protected by the
    /// 'RawBuffer' when split up.
    unsafe fn to_raw_parts(self) -> (RawBuffer, Self::Data);
}

pub struct MMAPBuffer {
    raw: RawBuffer,
    memory: MappedMemory,
}

impl Buffer for MMAPBuffer {
    type Data = MappedMemory;

    unsafe fn from_raw_parts(raw: RawBuffer, data: Self::Data) -> Self {
        Self { raw, memory: data }
    }

    unsafe fn to_raw_parts(self) -> (RawBuffer, Self::Data) {
        (self.raw, self.memory)
    }
}

impl MMAPBuffer {
    // TODO: Must interpret the plane 'data_offset' field.

    pub fn memory<'a>(&'a self) -> &'a [u8] {
        unsafe { core::slice::from_raw_parts(self.memory.addr(), self.memory.len()) }
    }

    /// NOTE: If you use this to fill the buffer, then call use_memory()
    /// afterwards to indicate how many bytes were written.
    pub fn memory_mut<'a>(&'a mut self) -> &'a mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.memory.addr(), self.memory.len()) }
    }

    pub fn use_memory<'a>(&'a mut self, len: usize) -> &'a mut [u8] {
        if v4l2_type_is_multiplane(v4l2_buf_type(self.raw.buffer.type_.into())) {
            self.raw.planes[0].bytesused = len as u32;
        } else {
            self.raw.buffer.bytesused = len as u32;
        }

        let buf = self.memory_mut();
        assert!(len <= buf.len());
        &mut buf[..len]
    }

    pub fn used_memory<'a>(&'a self) -> &'a [u8] {
        let n = {
            if v4l2_type_is_multiplane(v4l2_buf_type(self.raw.buffer.type_.into())) {
                self.raw.planes[0].bytesused
            } else {
                self.raw.buffer.bytesused
            }
        };

        let buf = self.memory();
        &buf[..(n as usize)]
    }

    pub fn sequence(&self) -> u32 {
        self.raw.buffer.sequence
    }

    pub fn monotonic_timestamp(&self) -> Option<Duration> {
        if self.raw.buffer.flags & crate::V4L2_BUF_FLAG_TIMESTAMP_MONOTONIC == 0 {
            return None;
        }

        let t = self.raw.buffer.timestamp;
        Some(Duration::from_secs(t.tv_sec as u64) + Duration::from_micros(t.tv_usec as u64))
    }
}

// TODO: Would I also need an offset?
pub trait DMABufferData {
    fn as_raw_fd(&self) -> i32;

    fn bytes_used(&self) -> usize;

    fn length(&self) -> usize;
}

pub struct DMABuffer<D> {
    raw: RawBuffer,
    data: Option<D>,
}

impl<D> DMABuffer<D> {
    pub fn take_data(&mut self) -> Option<D> {
        self.data.take()
    }

    pub fn set_data(&mut self, data: D) {
        self.data = Some(data);
    }
}

impl<D: DMABufferData> Buffer for DMABuffer<D> {
    type Data = Option<D>;

    unsafe fn from_raw_parts(raw: RawBuffer, data: Self::Data) -> Self {
        Self { raw, data }
    }

    unsafe fn to_raw_parts(self) -> (RawBuffer, Self::Data) {
        let mut raw = self.raw;
        let data = self.data;

        if v4l2_type_is_multiplane(v4l2_buf_type(raw.buffer.type_.into())) {
            if let Some(data) = &data {
                raw.planes[0].bytesused = data.bytes_used() as u32;
                raw.planes[0].length = data.length() as u32;
                raw.planes[0].m.fd = data.as_raw_fd();
            } else {
                raw.planes[0].bytesused = 0;
                raw.planes[0].length = 0;
                raw.planes[0].m.fd = 0;
            }
        } else {
            if let Some(data) = &data {
                raw.buffer.bytesused = data.bytes_used() as u32;
                raw.buffer.length = data.length() as u32;
                raw.buffer.m.fd = data.as_raw_fd();
            } else {
                raw.buffer.bytesused = 0;
                raw.buffer.length = 0;
                raw.buffer.m.fd = 0;
            }
        }

        (raw, data)
    }
}
