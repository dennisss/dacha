use std::pin::Pin;

use crate::bindings::Size;
use crate::ffi;
use crate::pixel_format::PixelFormat;
use crate::stream::Stream;
use crate::stream_formats::StreamFormats;

/// Wrapper around a ffi::StreamConfiguration value. Note that because CXX only
/// understands C++ structs as opaque values with no defined size, this struct
/// will never be instantiated. You will only ever get references to it.
#[repr(transparent)]
pub struct StreamConfiguration {
    config: ffi::StreamConfiguration,
}

impl StreamConfiguration {
    pub fn pixel_format(&self) -> PixelFormat {
        ffi::stream_config_pixel_format(&self.config)
    }

    pub fn set_pixel_format(&mut self, value: PixelFormat) {
        ffi::stream_config_set_pixel_format(unsafe { Pin::new_unchecked(&mut self.config) }, value)
    }

    pub fn size(&self) -> Size {
        ffi::stream_config_size(&self.config)
    }

    pub fn set_size(&mut self, value: Size) {
        ffi::stream_config_set_size(unsafe { Pin::new_unchecked(&mut self.config) }, value)
    }

    // TODO: Most of these fields are only valid after the config is validated.

    pub fn stride(&self) -> u32 {
        ffi::stream_config_stride(&self.config)
    }

    pub fn set_stride(&mut self, value: u32) {
        ffi::stream_config_set_stride(unsafe { Pin::new_unchecked(&mut self.config) }, value)
    }

    pub fn frame_size(&self) -> u32 {
        ffi::stream_config_frame_size(&self.config)
    }

    pub fn set_frame_size(&mut self, value: u32) {
        ffi::stream_config_set_frame_size(unsafe { Pin::new_unchecked(&mut self.config) }, value)
    }

    pub fn buffer_count(&self) -> u32 {
        ffi::stream_config_buffer_count(&self.config)
    }

    pub fn set_buffer_count(&mut self, value: u32) {
        ffi::stream_config_set_buffer_count(unsafe { Pin::new_unchecked(&mut self.config) }, value)
    }

    pub fn formats(&self) -> &StreamFormats {
        unsafe { core::mem::transmute(self.config.formats()) }
    }

    /// NOTE: Streams will only be non-None after the camera is configured.
    pub fn stream(&self) -> Option<&Stream> {
        let stream = self.config.stream();
        if stream != core::ptr::null_mut() {
            Some(unsafe { core::mem::transmute(stream) })
        } else {
            None
        }
    }
}

impl ToString for StreamConfiguration {
    fn to_string(&self) -> String {
        ffi::stream_config_to_string(&self.config)
    }
}
