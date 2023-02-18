use std::sync::Arc;

use cxx::UniquePtr;

use crate::camera::{AvailableCamera, Camera};
use crate::errors::*;
use crate::ffi;

pub struct CameraManager {
    raw: UniquePtr<ffi::CameraManager>,
}

impl CameraManager {
    /// Creates and starts a CameraManager instance.
    pub fn create() -> Result<Arc<Self>> {
        let mut raw = ffi::new_camera_manager();
        assert!(!raw.is_null());

        ok_if_zero(raw.as_mut().unwrap().start())?;

        Ok(Arc::new(Self { raw }))
    }

    /// NOTE: Some of these cameras may have already been acquired in previous
    /// calls to this function.
    pub fn cameras(self: &Arc<Self>) -> Vec<AvailableCamera> {
        let mut out = vec![];

        for camera in ffi::list_cameras(self.raw.as_ref().unwrap()) {
            out.push(AvailableCamera::new(Arc::new(Camera::new(
                self.clone(),
                camera.camera,
            ))));
        }

        out
    }
}

impl Drop for CameraManager {
    fn drop(&mut self) {
        self.raw.as_mut().unwrap().stop();
    }
}
