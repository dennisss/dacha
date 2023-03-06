use std::collections::HashMap;
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};

use cxx::UniquePtr;

use crate::camera::{Camera, RequestQueueEntry};
use crate::control_list::ControlList;
use crate::errors::*;
use crate::ffi;
use crate::ffi::{RequestReuseFlag, RequestStatus};
use crate::frame_buffer::FrameBuffer;
use crate::stream::Stream;

// TODO: Implement all debug impls using Request::toString()

/// NOTE: When this is dropped, the libcamers C++ code will cancel the request
/// in the destructor.
///
/// Will also get canclled by C++ if the camera is stopped.
pub struct Request {
    pub(crate) raw: UniquePtr<ffi::Request>,

    /// All the buffers associated with this request.
    /// Key is the stream id.
    buffers: HashMap<u64, FrameBuffer>,

    /// MUST be the last field in this struct to be dropped last.
    #[allow(unused)]
    camera: Arc<Camera>,
}

unsafe impl Send for Request {}
unsafe impl Sync for Request {}

impl Request {
    pub(crate) fn new(camera: Arc<Camera>, raw: UniquePtr<ffi::Request>) -> Self {
        Self {
            camera,
            raw,
            buffers: HashMap::new(),
        }
    }

    pub fn add_buffer(&mut self, buffer: FrameBuffer) -> Result<()> {
        assert!(self.camera.contains_stream(buffer.stream));

        // libcamera's addBuffer return EEXIST if the request already has a buffer
        // associated with the stream.
        ok_if_zero(unsafe {
            self.raw.as_mut().unwrap().addBuffer(
                buffer.stream.as_mut_ptr(),
                buffer.raw,
                UniquePtr::null(),
            )
        })?;

        self.buffers.insert(buffer.stream.id(), buffer);

        Ok(())
    }

    pub fn status(&self) -> RequestStatus {
        self.raw.status()
    }

    pub fn cookie(&self) -> u64 {
        self.raw.cookie()
    }

    // TODO: Change to read only and only allow on a completed request.
    pub fn metadata<'a>(&'a self) -> &'a ControlList {
        // Safe because on the C++ side, this is just a simple field accessor.
        unsafe {
            (Pin::new_unchecked(
                &mut *(core::mem::transmute::<&ffi::Request, u64>(self.raw.as_ref().unwrap())
                    as *mut ffi::Request),
            )
            .metadata()
            .get_unchecked_mut() as &ffi::ControlList)
                .into()
        }
    }

    pub fn controls_mut<'a>(&'a mut self) -> &'a mut ControlList {
        unsafe {
            self.raw
                .as_mut()
                .unwrap()
                .controls()
                .get_unchecked_mut()
                .into()
        }
    }
}

impl ToString for Request {
    fn to_string(&self) -> String {
        ffi::request_to_string(&self.raw)
    }
}

/// A request which has not yet been enqueued for execution (so is still
/// completely owned by the libcamera user).
pub struct NewRequest {
    request: Request,
}

impl NewRequest {
    pub(crate) fn new(request: Request) -> Self {
        Self { request }
    }

    /// Enqueue the request to be executed on the camera.
    ///
    /// NOTE: This may only be called when the camera is running.
    ///
    /// Ownership of memory associated with the request is transferred to
    /// libcamera internal threads.
    pub fn enqueue(mut self) -> Result<PendingRequest> {
        let camera = self.request.camera.clone();
        let entry = camera.queue_request(&mut self.request)?;

        Ok(PendingRequest {
            request: Some(self.request),
            entry,
        })
    }
}

impl Deref for NewRequest {
    type Target = Request;

    fn deref(&self) -> &Request {
        &self.request
    }
}

impl DerefMut for NewRequest {
    fn deref_mut(&mut self) -> &mut Request {
        &mut self.request
    }
}

/// A request which may still be executing on the camera.
/// In this state, the request data is owned by the libcamera internal threads.
///
/// TODO: Verify that if this is dropped before it finishes executing that we
/// clean up the entry in the camera.
pub struct PendingRequest {
    request: Option<Request>,
    entry: Arc<Mutex<RequestQueueEntry>>,
}

impl PendingRequest {
    /// If the request is done executing, gets a CompletedRequest value,
    /// otherwise, returns the same PendingRequest.
    pub fn try_complete(mut self) -> std::result::Result<CompletedRequest, PendingRequest> {
        if self.entry.lock().unwrap().done {
            Ok(CompletedRequest {
                request: self.request.take().unwrap(),
            })
        } else {
            Err(self)
        }
    }
}

impl Future for PendingRequest {
    type Output = CompletedRequest;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.entry.lock().unwrap();
        if state.done {
            drop(state);

            return Poll::Ready(CompletedRequest {
                request: self.request.take().unwrap(),
            });
        }

        state.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

pub struct CompletedRequest {
    request: Request,
}

impl Deref for CompletedRequest {
    type Target = Request;

    fn deref(&self) -> &Request {
        &self.request
    }
}

impl CompletedRequest {
    /// Re-uses the request object as a new request.
    ///
    /// Any buffers added at the request already will be retained for the new
    /// request (so additional buffers should not need to be added).
    pub fn reuse(mut self) -> NewRequest {
        self.request
            .raw
            .as_mut()
            .unwrap()
            .reuse(RequestReuseFlag::ReuseBuffers);

        NewRequest {
            request: self.request,
        }
    }

    pub fn buffer(&self, stream: &Stream) -> Option<&FrameBuffer> {
        self.request.buffers.get(&stream.id())
    }

    pub fn buffer_by_id(&self, stream_id: u64) -> Option<&FrameBuffer> {
        self.request.buffers.get(&stream_id)
    }

    pub fn sequence(&self) -> u32 {
        self.request.raw.sequence()
    }
}
