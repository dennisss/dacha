use core::fmt::Debug;

use crate::bindings::*;

/// NOTE: The raw v4l2_format struct can't be directly manipulated by users
/// since it is unsafe to use the incorrect single/multi-plane union case.
#[derive(Clone, Copy)]
pub struct Format {
    pub(crate) raw: v4l2_format,
}

unsafe impl Sync for Format {}
unsafe impl Send for Format {}

macro_rules! accessor {
    ($name:ident, $name_mut:ident, $t:ty, $field:ident) => {
        pub fn $name(&self) -> $t {
            if v4l2_type_is_multiplane(v4l2_buf_type(self.raw.type_)) {
                unsafe { self.raw.fmt.pix_mp.$field }
            } else {
                unsafe { self.raw.fmt.pix.$field }
            }
        }

        pub fn $name_mut(&mut self, value: $t) {
            if v4l2_type_is_multiplane(v4l2_buf_type(self.raw.type_)) {
                unsafe {
                    self.raw.fmt.pix_mp.$field = value;
                }
            } else {
                unsafe {
                    self.raw.fmt.pix.$field = value;
                }
            }
        }
    };
}

impl Format {
    accessor!(width, set_width, u32, width);
    accessor!(height, set_height, u32, height);
    accessor!(pixelformat, set_pixelformat, u32, pixelformat);
    accessor!(field, set_field, u32, field);
    accessor!(colorspace, set_colorspace, u32, colorspace);
    // accessor!(xfer_func, set_xfer_func, u32, xfer_func);

    pub fn set_xfer_func(&mut self, value: u32) {
            if v4l2_type_is_multiplane(v4l2_buf_type(self.raw.type_)) {
                unsafe {
                    self.raw.fmt.pix_mp.xfer_func = value as u8;
                }
            } else {
                unsafe {
                    self.raw.fmt.pix.xfer_func = value;
                }
            }

    }

    /// Gets the maximum number of planes that this struct/stream type supports
    /// defining. Does not check if the pixelformat can handle this many planes.
    pub fn max_num_planes(&self) -> usize {
        if v4l2_type_is_multiplane(v4l2_buf_type(self.raw.type_)) {
            unsafe { self.raw.fmt.pix_mp.plane_fmt.len() }
        } else {
            1
        }
    }

    pub fn set_num_planes(&mut self, num: usize) {
        assert!(num <= self.max_num_planes());
        if v4l2_type_is_multiplane(v4l2_buf_type(self.raw.type_)) {
            self.raw.fmt.pix_mp.num_planes = 1;
        }
    }

    pub fn set_plane_format(&mut self, index: usize, format: v4l2_plane_pix_format) {
        if v4l2_type_is_multiplane(v4l2_buf_type(self.raw.type_)) {
            unsafe { self.raw.fmt.pix_mp.plane_fmt[index] = format };
        } else {
            assert!(index == 0);
            unsafe {
                self.raw.fmt.pix.bytesperline = format.bytesperline;
                self.raw.fmt.pix.sizeimage = format.sizeimage;
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FormatDefinition {
    pub description: String,
    pub flags: u32,
    pub pixelformat: PixelFormat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PixelFormat(pub u32);

impl PixelFormat {
    pub fn to_string(&self) -> String {
        bytes_to_string(&self.0.to_le_bytes())
    }
}

impl Debug for PixelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\"", self.to_string())
    }
}

// TODO: Deduplicate this logic.
fn bytes_to_string(input: &[u8]) -> String {
    let mut out = String::new();
    out.reserve(input.len());

    for b in input {
        if b.is_ascii_graphic() || *b == b' ' {
            out.push(*b as char);
        } else {
            out.push_str(&format!("\\x{:02x}", b));
        }
    }

    out
}
