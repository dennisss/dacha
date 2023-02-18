use crate::bindings::{Size, SizeRange};
use crate::ffi;
use crate::pixel_format::PixelFormat;

#[repr(transparent)]
pub struct StreamFormats {
    raw: ffi::StreamFormats,
}

impl StreamFormats {
    pub fn pixel_formats(&self) -> Vec<PixelFormat> {
        ffi::stream_formats_pixelformats(&self.raw)
            .into_iter()
            .map(|v| v.value)
            .collect()
    }

    pub fn sizes(&self, pixel_format: PixelFormat) -> Vec<Size> {
        ffi::stream_formats_sizes(&self.raw, &pixel_format.into())
            .into_iter()
            .map(|v| v.value)
            .collect()
    }

    pub fn range(&self, pixel_format: PixelFormat) -> SizeRange {
        self.raw.range(&pixel_format.into())
    }
}
