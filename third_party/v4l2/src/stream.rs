use std::{collections::HashSet, sync::Arc};

use base_error::*;
use executor::lock;
use executor::sync::AsyncMutex;
use file::{LocalFile, LocalFileOpenOptions, LocalPath};
use sys::Errno;
use sys::MappedMemory;

use crate::bindings::*;
use crate::buffer::*;
use crate::device::DeviceHandle;
use crate::io::*;

// TODO: On drop, consider turning the stream off and deallocating all buffers?
pub struct UnconfiguredStream {
    pub(crate) device: Arc<DeviceHandle>,
    pub(crate) typ: v4l2_buf_type,
}

impl UnconfiguredStream {
    pub async fn get_format(&self) -> Result<v4l2_format> {
        let dev = self.device.shared.file.lock().await?.read_exclusive();
        self.get_format_impl(&dev)
    }

    fn get_format_impl(&self, file: &LocalFile) -> Result<v4l2_format> {
        let mut format = v4l2_format::default();
        format.type_ = self.typ.0;
        unsafe { vidioc_g_fmt(file.as_raw_fd(), &mut format) }?;
        Ok(format)
    }

    // TODO: This is potentially unsafe if we mis-match multi-plane formats with
    // non-multi-plane types.
    pub async fn set_format(&mut self, mut format: v4l2_format) -> Result<()> {
        let dev = self.device.shared.file.lock().await?.read_exclusive();
        format.type_ = self.typ.0;
        unsafe { vidioc_s_fmt(dev.as_raw_fd(), &mut format) }?;
        Ok(())
    }

    pub async fn get_streaming_params(&self) -> Result<v4l2_streamparm> {
        let dev = self.device.shared.file.lock().await?.read_exclusive();

        let mut param = v4l2_streamparm::default();
        param.type_ = self.typ.0;
        unsafe { vidioc_g_parm(dev.as_raw_fd(), &mut param) }?;
        Ok(param)
    }

    // TOOD: This is unsafe if we mis-match the 'output' or 'capture' params with
    // the type of the stream.
    pub async fn set_streaming_params(&mut self, mut param: v4l2_streamparm) -> Result<()> {
        let dev = self.device.shared.file.lock().await?.read_exclusive();
        param.type_ = self.typ.0;
        unsafe { vidioc_s_parm(dev.as_raw_fd(), &mut param) }?;
        Ok(())
    }

    /// Creates a new buffer with newly allocated memory (allocated by the
    /// driver) which is mmap'ed into the current process.
    pub async fn configure_mmap(
        mut self,
        num_buffers: usize,
    ) -> Result<(Stream<MMAPBuffer>, Vec<MMAPBuffer>)> {
        let dev = self.device.shared.file.lock().await?.read_exclusive();

        let format = self.get_format_impl(&dev)?;
        let num_planes = {
            if v4l2_type_is_multiplane(self.typ) {
                unsafe { format.fmt.pix_mp.num_planes as usize }
            } else {
                0
            }
        };

        if num_planes > 1 {
            return Err(err_msg("Buffers with >1 planes are not supported"));
        }

        // NOTE: vidioc_reqbufs will alter the # of registered buffers if some where
        // already allocated. This may fail if some buffers are in use so we avoid using
        // this behavior by preventing a stream from being configured twice.
        let mut request = v4l2_requestbuffers::default();
        request.type_ = self.typ.0;
        request.memory = v4l2_memory::V4L2_MEMORY_MMAP.0;
        request.count = num_buffers as u32;
        unsafe { vidioc_reqbufs(dev.as_raw_fd(), &mut request) }
            .map_err(|e| format_err!("vidioc_reqbufs failed: {}", e))?;

        let mut buffers = vec![];
        buffers.reserve_exact(num_buffers);

        for i in 0..num_buffers {
            let mut planes = vec![];

            let mut buffer = v4l2_buffer::default();
            buffer.type_ = self.typ.0;
            buffer.memory = v4l2_memory::V4L2_MEMORY_MMAP.0;
            buffer.index = i as u32; // Index relative to the # we requested.

            if v4l2_type_is_multiplane(self.typ) {
                let n = unsafe { format.fmt.pix_mp.num_planes as usize };
                planes.reserve_exact(n);
                planes.resize(n, v4l2_plane::default());

                buffer.length = n as u32; // # of planes
                buffer.m.planes = planes.as_mut_ptr();
            }

            unsafe { vidioc_querybuf(dev.as_raw_fd(), &mut buffer) }
                .map_err(|e| format_err!("vidioc_querybuf failed: {}", e))?;

            let (offset, length) = unsafe {
                if v4l2_type_is_multiplane(self.typ) {
                    assert!(planes.len() == 1);
                    (planes[0].m.mem_offset, planes[0].length)
                } else {
                    (buffer.m.offset, buffer.length)
                }
            };

            let memory = unsafe {
                MappedMemory::create(
                    core::ptr::null_mut(),
                    length as usize,
                    sys::bindings::PROT_READ | sys::bindings::PROT_WRITE,
                    sys::bindings::MAP_SHARED,
                    dev.as_raw_fd(),
                    offset as usize,
                )
                .map_err(|e| format_err!("mmap failed: {}", e))?
            };

            let raw = RawBuffer {
                device: self.device.clone(),
                buffer,
                planes,
            };

            buffers.push(unsafe { MMAPBuffer::from_raw_parts(raw, memory) });
        }

        drop(dev);

        let mut enqueued_buffers = vec![];
        enqueued_buffers.resize_with(num_buffers, || None);

        let inst = Stream {
            device: self.device,
            typ: self.typ,
            memory_typ: v4l2_memory::V4L2_MEMORY_MMAP,
            num_planes,
            enqueued_buffers: AsyncMutex::new(enqueued_buffers),
        };

        Ok((inst, buffers))
    }

    pub async fn configure_dma<D: DMABufferData>(
        mut self,
        num_buffers: usize,
    ) -> Result<(Stream<DMABuffer<D>>, Vec<DMABuffer<D>>)> {
        let dev = self.device.shared.file.lock().await?.read_exclusive();

        let format = self.get_format_impl(&dev)?;
        let num_planes = {
            if v4l2_type_is_multiplane(self.typ) {
                unsafe { format.fmt.pix_mp.num_planes as usize }
            } else {
                0
            }
        };

        if num_planes > 1 {
            return Err(err_msg("Buffers with >1 planes are not supported"));
        }

        let mut request = v4l2_requestbuffers::default();
        request.type_ = self.typ.0;
        request.memory = v4l2_memory::V4L2_MEMORY_DMABUF.0;
        request.count = num_buffers as u32;
        unsafe { vidioc_reqbufs(dev.as_raw_fd(), &mut request) }?;

        drop(dev);

        let mut enqueued_buffers = vec![];
        enqueued_buffers.resize_with(num_buffers, || None);

        let mut buffers = vec![];
        buffers.reserve_exact(num_buffers);

        for i in 0..num_buffers {
            let mut planes = vec![];
            planes.reserve_exact(num_planes);
            planes.resize(num_planes, v4l2_plane::default());

            let mut buffer = v4l2_buffer::default();
            buffer.type_ = self.typ.0;
            buffer.memory = v4l2_memory::V4L2_MEMORY_DMABUF.0;
            buffer.index = i as u32; // Index relative to the # we requested.

            if v4l2_type_is_multiplane(self.typ) {
                buffer.length = num_planes as u32; // # of planes
                buffer.m.planes = planes.as_mut_ptr();
            }

            let raw = RawBuffer {
                device: self.device.clone(),
                buffer,
                planes,
            };

            buffers.push(unsafe { DMABuffer::from_raw_parts(raw, None) });
        }

        let inst = Stream {
            device: self.device,
            typ: self.typ,
            memory_typ: v4l2_memory::V4L2_MEMORY_DMABUF,
            num_planes,
            enqueued_buffers: AsyncMutex::new(enqueued_buffers),
        };

        Ok((inst, buffers))
    }
}

pub struct Stream<B: Buffer> {
    device: Arc<DeviceHandle>,
    typ: v4l2_buf_type,
    memory_typ: v4l2_memory,

    /// Number of planes in this each buffer.
    /// Zero if using a single-planar mode.
    num_planes: usize,

    /// User data associated with buffers.
    ///
    /// This is always the same length as the number of allocated streams.
    ///
    /// If None, then the buffer is currently owned by the user.
    enqueued_buffers: AsyncMutex<Vec<Option<B::Data>>>,
}

// TODO: Instead wrap the Buffer in a safe combination with a plaen.
unsafe impl Send for v4l2_buffer {}
unsafe impl Sync for v4l2_buffer {}

impl<B: Buffer> Stream<B> {
    /// NOTE: If no buffers are available to be dequeued, this will block until
    /// one is available.
    ///
    /// TODO: Require '&mut self' for this (TODO: Instead allow splitting
    /// streams in half)
    pub async fn dequeue_buffer(&self) -> Result<B> {
        // NOTE: This must be pinned as we make a reference to it below.
        let mut planes = vec![];
        planes.reserve_exact(self.num_planes);
        planes.resize(self.num_planes, v4l2_plane::default());

        let mut buffer = v4l2_buffer::default();
        buffer.type_ = self.typ.0;
        buffer.memory = self.memory_typ.0;
        buffer.index = u32::MAX; // Index relative to the # we requested.

        if v4l2_type_is_multiplane(self.typ) {
            buffer.length = self.num_planes as u32;
            buffer.m.planes = planes.as_mut_ptr();
        }

        loop {
            let dev = self.device.shared.file.lock().await?.read_exclusive();

            match unsafe { vidioc_dqbuf(dev.as_raw_fd(), &mut buffer) } {
                Ok(_) => break,
                Err(Errno::EAGAIN) => {}
                Err(e) => return Err(e.into()),
            };

            dev.wait().await;
        }

        let data = lock!(enqueued_buffers <= self.enqueued_buffers.lock().await?, {
            enqueued_buffers[buffer.index as usize].take().unwrap()
        });

        let raw = RawBuffer {
            device: self.device.clone(),
            buffer,
            planes,
        };

        // TODO: Only consider this if we care about reading the contents of the buffer
        // (or feeding it as an input to a stream that will read from it).
        if raw.buffer.flags & V4L2_BUF_FLAG_ERROR != 0 {
            return Err(err_msg("V4L2 buffer returned with corrupted data"));
        }

        Ok(unsafe { B::from_raw_parts(raw, data) })
    }

    /// NOTE: This should always execute quickly without much blocking.
    ///
    /// TODO: Require '&mut self' for this (we want to ensure that data is
    /// processed in the order it is send)
    pub async fn enqueue_buffer(&self, buffer: B) -> Result<()> {
        let (mut raw, data) = unsafe { buffer.to_raw_parts() };
        let index = raw.buffer.index as usize;

        // The buffer must have been generated by the same device/stream.
        assert!(core::ptr::eq::<DeviceHandle>(
            raw.device.as_ref(),
            self.device.as_ref()
        ));
        assert_eq!(raw.buffer.type_, self.typ.0);

        let dev = self.device.shared.file.lock().await?.read_exclusive();

        lock!(enqueued_buffers <= self.enqueued_buffers.lock().await?, {
            assert!(enqueued_buffers[index].is_none());

            unsafe { vidioc_qbuf(dev.as_raw_fd(), &mut raw.buffer) }?;

            enqueued_buffers[index] = Some(data);

            Ok::<(), Error>(())
        })?;

        Ok(())
    }

    /// TODO: Consider making this return a different 'Stream' type.
    pub async fn turn_on(&mut self) -> Result<()> {
        let dev = self.device.shared.file.lock().await?.read_exclusive();
        unsafe { vidioc_streamon(dev.as_raw_fd(), &self.typ.0) }?;
        Ok(())
    }

    /*
    TODO: VIDIOC_STREAMOFF
    - This will also auto-dequeue all buffers.

    */
}
