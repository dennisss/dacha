use std::collections::HashMap;
use std::ops::Deref;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::Waker;

use cxx::SharedPtr;
use cxx::UniquePtr;

use crate::bindings::StreamRole;
use crate::camera_configuration::*;
use crate::camera_manager::CameraManager;
use crate::control_info_map::ControlInfoMap;
use crate::control_list::ControlList;
use crate::errors::*;
use crate::ffi;
use crate::frame_buffer_allocator::FrameBufferAllocator;
use crate::request::{NewRequest, Request};
use crate::stream::Stream;

// TODO: On drop, do release/stop?

pub struct Camera {
    raw: SharedPtr<ffi::Camera>,

    state: Arc<Mutex<CameraState>>,

    /// Used to ensure that the ffi::Camera outlives the ffi::CameraManager.
    ///
    /// MUST be the last field in this struct to be dropped last.
    #[allow(unused)]
    manager: Arc<CameraManager>,
}

unsafe impl Send for Camera {}
unsafe impl Sync for Camera {}

struct CameraState {
    /// When in the 'Running' state, this will store a map of requests which
    /// have been enqueued to run but are not yet complete.
    ///
    /// The key is each request's pointer.
    pending_requests: HashMap<u64, Arc<Mutex<RequestQueueEntry>>>,
}

pub(crate) struct RequestQueueEntry {
    /// If true, the request was either cancelled or completed.
    pub done: bool,

    pub waker: Option<Waker>,
}

// NOTE: Most shared logic is stored in private methods here. They should be
// exposed as public methods in the appropriate state specific structs if they
// are valid to be called in that state.
impl Camera {
    pub(crate) fn new(manager: Arc<CameraManager>, raw: SharedPtr<ffi::Camera>) -> Self {
        Self {
            manager,
            raw,
            state: Arc::new(Mutex::new(CameraState {
                pending_requests: HashMap::new(),
            })),
        }
    }

    pub fn id(&self) -> String {
        self.raw.as_ref().unwrap().id().to_string()
    }

    pub fn streams<'a>(&'a self) -> Vec<&'a Stream> {
        ffi::camera_streams(self.raw.as_ref().unwrap())
            .into_iter()
            .map(|v| unsafe { core::mem::transmute(v.stream) })
            .collect()
    }

    // TODO: Instead of handling out &Stream's, we should just hand out ids.
    // TODO: If we configure it with more roles, will that make new streams?
    pub(crate) fn contains_stream(&self, stream: &Stream) -> bool {
        unsafe { ffi::camera_contains_stream(self.raw.as_ref().unwrap(), stream.as_mut_ptr()) }
    }

    pub fn controls<'a>(&'a self) -> &'a ControlInfoMap {
        self.raw.controls().into()
    }

    pub fn properties<'a>(&'a self) -> &'a ControlList {
        self.raw.properties().into()
    }

    fn get_mut(&self) -> Pin<&mut ffi::Camera> {
        unsafe {
            Pin::<&mut ffi::Camera>::new_unchecked(
                &mut *(core::mem::transmute::<_, u64>(self.raw.as_ref().unwrap())
                    as *mut ffi::Camera),
            )
        }
    }

    fn acquire(&self) -> Result<()> {
        ok_if_zero(self.get_mut().acquire())
    }

    fn release(&self) -> Result<()> {
        ok_if_zero(self.get_mut().release())
    }

    fn generate_configuration(
        self: &Arc<Self>,
        stream_roles: &[StreamRole],
    ) -> Option<CameraConfiguration> {
        let raw = ffi::generate_camera_configuration(self.get_mut(), stream_roles);
        if raw.is_null() {
            return None;
        }

        Some(CameraConfiguration::new(self.clone(), raw))
    }

    // Only allowed for Configured and Running cameras
    //
    // TODO: What should we do with requests that are still hanging when the camera
    // is stopped.
    fn create_request(self: &Arc<Self>, cookie: u64) -> NewRequest {
        let raw = self.get_mut().createRequest(cookie);
        assert!(!raw.is_null());
        NewRequest::new(Request::new(self.clone(), raw))
    }

    pub(crate) fn queue_request(
        &self,
        request: &mut Request,
    ) -> Result<Arc<Mutex<RequestQueueEntry>>> {
        // NOTE: We lock before enqueuing to prevent the race condition of receiving a
        // completion event before the request is fully enqueued.
        let mut state = self.state.lock().unwrap();

        // We assume that in C++, queueRequest will return an error if the camera isn't
        // in a Running state.
        ok_if_zero(unsafe {
            self.get_mut()
                .queueRequest(request.raw.as_mut().unwrap().get_unchecked_mut())
        })?;

        let entry = Arc::new(Mutex::new(RequestQueueEntry {
            done: false,
            waker: None,
        }));

        let request_id =
            unsafe { core::mem::transmute::<&ffi::Request, _>(request.raw.as_ref().unwrap()) };

        assert!(!state.pending_requests.contains_key(&request_id));
        state.pending_requests.insert(request_id, entry.clone());

        Ok(entry)
    }
}

/// A reference to a camera which may be acquired for exclusive access.
pub struct AvailableCamera {
    camera: Arc<Camera>,
}

impl Deref for AvailableCamera {
    type Target = Camera;

    fn deref(&self) -> &Camera {
        &self.camera
    }
}

impl AvailableCamera {
    pub(crate) fn new(camera: Arc<Camera>) -> Self {
        Self { camera }
    }

    pub fn acquire(self) -> Result<AcquiredCamera> {
        self.camera.acquire()?;
        Ok(AcquiredCamera {
            camera: self.camera,
        })
    }
}

pub struct AcquiredCamera {
    camera: Arc<Camera>,
}

impl Deref for AcquiredCamera {
    type Target = Camera;

    fn deref(&self) -> &Camera {
        &self.camera
    }
}

impl AcquiredCamera {
    pub fn release(self) -> Result<AvailableCamera> {
        self.camera.release()?;
        Ok(AvailableCamera {
            camera: self.camera,
        })
    }

    pub fn generate_configuration(
        &self,
        stream_roles: &[StreamRole],
    ) -> Option<CameraConfiguration> {
        self.camera.generate_configuration(stream_roles)
    }

    pub fn configure(self, config: &mut CameraConfiguration) -> Result<ConfiguredCamera> {
        ok_if_zero(unsafe {
            self.camera
                .get_mut()
                .configure(config.raw.as_mut().unwrap().get_unchecked_mut())
        })?;

        Ok(ConfiguredCamera {
            camera: self.camera,
        })
    }
}

pub struct ConfiguredCamera {
    camera: Arc<Camera>,
}

impl Deref for ConfiguredCamera {
    type Target = Camera;

    fn deref(&self) -> &Camera {
        &self.camera
    }
}

impl ConfiguredCamera {
    pub fn new_frame_buffer_allocator(&self) -> FrameBufferAllocator {
        let raw = ffi::new_frame_buffer_allocator(self.camera.raw.clone());
        assert!(!raw.is_null());

        FrameBufferAllocator::new(self.camera.clone(), raw)
    }

    pub fn create_request(&self, cookie: u64) -> NewRequest {
        self.camera.create_request(cookie)
    }

    pub fn start(self, control_list: Option<&ControlList>) -> Result<RunningCamera> {
        let control_list = match control_list {
            Some(v) => unsafe { core::mem::transmute(v) },
            None => core::ptr::null(),
        };

        ok_if_zero(unsafe { self.camera.get_mut().start(control_list) })?;
        RunningCamera::create(self.camera)
    }
}

pub struct RunningCamera {
    camera: Arc<Camera>,

    /// While the camera is running, we keep a listener connected to the
    /// requestCompleted signal.
    #[allow(unused)]
    request_complete_slot: UniquePtr<ffi::RequestCompleteSlot>,
}

unsafe impl Send for RunningCamera {}
unsafe impl Sync for RunningCamera {}

impl Deref for RunningCamera {
    type Target = Camera;

    fn deref(&self) -> &Camera {
        &self.camera
    }
}

impl RunningCamera {
    fn create(camera: Arc<Camera>) -> Result<Self> {
        let state = camera.state.clone();
        let request_complete_slot = ffi::camera_connect_request_completed(
            camera.get_mut(),
            |ctx, req| {
                (ctx.handler)(req);
            },
            Box::new(ffi::RequestCompleteContext {
                handler: Box::new(move |req| Self::handle_request_complete(&state, req)),
            }),
        );

        Ok(RunningCamera {
            camera,
            request_complete_slot,
        })
    }

    fn handle_request_complete(state: &Arc<Mutex<CameraState>>, request: &ffi::Request) {
        let mut state = state.lock().unwrap();

        let request_id = unsafe { core::mem::transmute::<&ffi::Request, _>(request) };

        let entry = state.pending_requests.remove(&request_id).unwrap();
        let mut guard = entry.lock().unwrap();

        guard.done = true;
        if let Some(waker) = guard.waker.take() {
            waker.wake();
        }
    }

    pub fn create_request(&self, cookie: u64) -> NewRequest {
        self.camera.create_request(cookie)
    }

    // TODO: Verify that when stopped, all requests get marked as cancelled.
}
