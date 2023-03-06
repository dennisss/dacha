use std::fmt::Debug;

use crate::control_value::ControlValue;
use crate::ffi;

pub use ffi::ControlInfo;

impl ControlInfo {
    pub fn values(&self) -> impl Iterator<Item = &ControlValue> {
        self.values_raw().iter()
    }
}

impl Debug for ControlInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} (default: {:?})",
            ffi::control_info_to_string(&self),
            self.def()
        )
    }
}
