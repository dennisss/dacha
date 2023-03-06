use std::{collections::HashSet, sync::Arc};

use base_error::*;
use executor::sync::Mutex;
use file::{LocalFile, LocalFileOpenOptions, LocalPath};
use sys::MappedMemory;

use crate::bindings::*;
use crate::io::*;
use crate::DeviceHandle;

/// Wrapper around a v4l2_buffer struct containing the current state of a buffer
/// which is currently owned by userspace.
///
/// Internally this assuems that a multi-plane stream with one plane is being
/// used.
pub struct RawBuffer {
    /// Maintain a reference to the device to ensure that the buffer outlives
    /// the
    pub(crate) device: Arc<DeviceHandle>,

    pub(crate) buffer: v4l2_buffer,
    pub(crate) plane: Box<v4l2_plane>,
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
    pub fn memory<'a>(&'a self) -> &'a [u8] {
        unsafe { core::slice::from_raw_parts(self.memory.addr(), self.memory.len()) }
    }

    pub fn memory_mut<'a>(&'a mut self) -> &'a mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.memory.addr(), self.memory.len()) }
    }

    pub fn use_memory<'a>(&'a mut self, len: usize) -> &'a mut [u8] {
        self.raw.plane.bytesused = len as u32;

        let buf = self.memory_mut();
        assert!(len <= buf.len());
        &mut buf[..len]
    }

    pub fn used_memory<'a>(&'a self) -> &'a [u8] {
        let buf = self.memory();
        &buf[..(self.raw.plane.bytesused as usize)]
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

        if let Some(data) = &data {
            raw.plane.bytesused = data.bytes_used() as u32;
            raw.plane.length = data.length() as u32;
            raw.plane.m.fd = data.as_raw_fd();
        } else {
            raw.plane.bytesused = 0;
            raw.plane.length = 0;
            raw.plane.m.fd = 0;
        }

        (raw, data)
    }
}
