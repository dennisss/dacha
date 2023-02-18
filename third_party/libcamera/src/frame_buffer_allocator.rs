use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use cxx::UniquePtr;

use crate::camera::Camera;
use crate::errors::*;
use crate::ffi;
use crate::frame_buffer::FrameBuffer;
use crate::stream::Stream;

/// Allocates FrameBuffers for storing camera response data.
///
/// NOTE: Compared to the C++ API, we do not allow explicitly freeing buffers
/// associated with a specific stream. Exposing this would add the risk of
/// freeing buffers still associated with a request. Instead all buffers in the
/// FrameBufferAllocator will be freed at once when all references to them are
/// dropped. If you need to create more buffers earlier than than, prefer to use
/// a different FrameBufferAllocator instance.
pub struct FrameBufferAllocator {
    inner: Arc<FrameBufferAllocatorInner>,
}

pub(crate) struct FrameBufferAllocatorInner {
    /// Ensure that the allocator outlives the camera (as it contains Stream
    /// references owned by the camera).
    #[allow(unused)]
    camera: Arc<Camera>,

    state: Mutex<FrameBufferAllocatorState>,
}

struct FrameBufferAllocatorState {
    raw: UniquePtr<ffi::FrameBufferAllocator>,
}

impl FrameBufferAllocator {
    pub(crate) fn new(camera: Arc<Camera>, raw: UniquePtr<ffi::FrameBufferAllocator>) -> Self {
        Self {
            inner: Arc::new(FrameBufferAllocatorInner {
                camera,
                state: Mutex::new(FrameBufferAllocatorState { raw }),
            }),
        }
    }

    pub fn allocate(&mut self, stream: &Stream) -> Result<Vec<FrameBuffer>> {
        assert!(self.inner.camera.contains_stream(stream));

        let mut state = self.inner.state.lock().unwrap();

        // NOTE: libcamera will return EBUSY if buffers have already been allocated for
        // the given stream so old buffers don't be overriden/freed.
        let n = to_result(unsafe { state.raw.as_mut().unwrap().allocate(stream.as_mut_ptr()) })?
            as usize;

        let buffers = unsafe { ffi::get_allocated_frame_buffers(&state.raw, stream.as_mut_ptr()) }
            .into_iter()
            .map(|b| FrameBuffer::new(self.inner.clone(), stream, b.buffer))
            .collect::<Vec<_>>();

        assert_eq!(buffers.len(), n);

        Ok(buffers)
    }
}
