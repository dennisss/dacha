use std::sync::Arc;

use crate::camera::Camera;
use crate::ffi;

// TODO: If a camera is re-configured, does that mean that old stream objects
// will dis-appear.

#[repr(transparent)]
pub struct Stream {
    pub(crate) raw: ffi::Stream,
}

impl Stream {
    pub fn id(&self) -> u64 {
        unsafe { core::mem::transmute(&self.raw) }
    }

    pub(crate) fn as_mut_ptr(&self) -> *mut ffi::Stream {
        unsafe { core::mem::transmute::<_, u64>(&self.raw) as *mut ffi::Stream }
    }

    pub(crate) unsafe fn as_static(&self) -> &'static Self {
        core::mem::transmute(self)
    }
}
