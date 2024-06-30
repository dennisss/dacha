use crate::bindings::*;

/// NOTE: The raw v4l2_format struct can't be directly manipulated by users
/// since it is unsafe to use the incorrect single/multi-plane union case.
#[derive(Clone, Copy)]
pub struct Format {
    pub(crate) raw: v4l2_format,
}

unsafe impl Sync for Format {}
unsafe impl Send for Format {}

impl Format {
    pub fn width(&self) -> u32 {
        // In the same memory position for both single/mplane formats.
        unsafe { self.raw.fmt.pix.width }
    }

    pub fn height(&self) -> u32 {
        // In the same memory position for both single/mplane formats.
        unsafe { self.raw.fmt.pix.height }
    }

    pub fn pixelformat(&self) -> u32 {
        // In the same memory position for both single/mplane formats.
        unsafe { self.raw.fmt.pix.pixelformat }
    }
}

#[derive(Clone, Debug)]
pub struct FormatDefinition {
    pub description: String,
    pub flags: u32,
    pub pixelformat: u32,
}
