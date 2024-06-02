use crate::bindings::*;

pub struct Format {
    pub(crate) raw: v4l2_format,
}

unsafe impl Sync for Format {}
unsafe impl Send for Format {}

impl Format {
    pub fn width(&self) -> u32 {
        // In the safe memory position for both single/mplane formats.
        unsafe { self.raw.fmt.pix.width }
    }

    pub fn height(&self) -> u32 {
        // In the safe memory position for both single/mplane formats.
        unsafe { self.raw.fmt.pix.height }
    }

    pub fn pixelformat(&self) -> u32 {
        // In the safe memory position for both single/mplane formats.
        unsafe { self.raw.fmt.pix.pixelformat }
    }
}

// v4l2_type_is_multiplane(self.typ)
