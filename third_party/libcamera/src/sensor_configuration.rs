pub use crate::bindings::SensorConfiguration;

impl Default for SensorConfiguration {
    fn default() -> Self {
        crate::ffi::new_sensor_config()
    }
}
