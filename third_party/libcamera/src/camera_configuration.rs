use std::sync::Arc;

use cxx::UniquePtr;

use crate::camera::Camera;
use crate::ffi;
use crate::stream_configuration::StreamConfiguration;

pub use crate::ffi::CameraConfigurationStatus;

pub struct CameraConfiguration {
    /// Used to ensure that the ffi::CameraConfiguration outlives and
    /// ffi::Camera.
    #[allow(unused)]
    camera: Arc<Camera>,

    /// This is public to allow the Camera to configure itself.
    pub(crate) raw: UniquePtr<ffi::CameraConfiguration>,
}

impl CameraConfiguration {
    pub(crate) fn new(camera: Arc<Camera>, raw: UniquePtr<ffi::CameraConfiguration>) -> Self {
        Self { camera, raw }
    }

    pub fn stream_configs_len(&self) -> usize {
        self.raw.as_ref().unwrap().size()
    }

    pub fn stream_config<'a>(&'a self, index: usize) -> &'a StreamConfiguration {
        unsafe { core::mem::transmute(self.raw.as_ref().unwrap().at(index as u32)) }
    }

    pub fn stream_config_mut<'a>(&'a mut self, index: usize) -> &'a mut StreamConfiguration {
        unsafe { core::mem::transmute(self.raw.as_mut().unwrap().at_mut(index as u32)) }
    }

    pub fn validate(&mut self) -> CameraConfigurationStatus {
        self.raw.as_mut().unwrap().validate()
    }
}
