use std::sync::Arc;

use cxx::UniquePtr;

use crate::camera::Camera;
use crate::ffi;
use crate::stream_configuration::StreamConfiguration;

pub use crate::ffi::CameraConfigurationStatus;
use crate::SensorConfiguration;

pub struct CameraConfiguration {
    /// This is public to allow the Camera to configure itself.
    pub(crate) raw: UniquePtr<ffi::CameraConfiguration>,

    /// Used to ensure that the ffi::CameraConfiguration outlives and
    /// ffi::Camera.
    ///
    /// MUST be the last field in this struct to be dropped last.
    #[allow(unused)]
    camera: Arc<Camera>,
}

unsafe impl Send for CameraConfiguration {}
unsafe impl Sync for CameraConfiguration {}

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

    pub fn sensor_config(&self) -> Option<SensorConfiguration> {
        if ffi::camera_config_has_sensor_config(&self.raw) {
            Some(ffi::camera_config_sensor_config(&self.raw))
        } else {
            None
        }
    }

    pub fn set_sensor_config(&mut self, value: Option<SensorConfiguration>) {
        let raw = self.raw.as_mut().unwrap();

        if let Some(value) = value {
            ffi::camera_config_set_sensor_config(raw, value);
        } else {
            ffi::camera_config_clear_sensor_config(raw);
        }
    }
}
