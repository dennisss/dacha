use std::fmt::Debug;

use crate::control_id::ControlId;
use crate::control_info::ControlInfo;
use crate::ffi;

#[repr(transparent)]
pub struct ControlInfoMap {
    raw: ffi::ControlInfoMap,
}

impl<'a> From<&'a ffi::ControlInfoMap> for &'a ControlInfoMap {
    fn from(value: &ffi::ControlInfoMap) -> Self {
        unsafe { core::mem::transmute(value) }
    }
}

impl ControlInfoMap {
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a ControlId, &'a ControlInfo)> {
        ffi::control_info_map_entries(&self.raw)
            .into_iter()
            .map(|entry| (entry.key, entry.value.into()))
    }
}

impl Debug for ControlInfoMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut s = f.debug_struct("ControlInfoMap");

        for entry in ffi::control_info_map_entries(&self.raw) {
            s.field(
                // Unwrap ok here as control names are always statically defined human readable
                // strings.
                entry.key.name().to_str().unwrap(),
                entry.value,
            );
        }

        s.finish()
    }
}
