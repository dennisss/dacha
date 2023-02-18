use std::fmt::Debug;

use crate::{bindings, ffi};

pub use bindings::PixelFormat;

impl ToString for PixelFormat {
    fn to_string(&self) -> String {
        ffi::pixel_format_to_string(self)
    }
}

impl Debug for PixelFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}
