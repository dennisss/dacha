mod camera;
mod camera_configuration;
mod camera_manager;
mod color_space;
mod control;
mod control_id;
mod control_info;
mod control_info_map;
mod control_list;
mod control_value;
mod errors;
mod ffi;
mod frame_buffer;
mod frame_buffer_allocator;
mod pixel_format;
mod request;
mod sensor_configuration;
mod stream;
mod stream_configuration;
mod stream_formats;

mod bindings {
    //! Bindgen produced bindings.

    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(unused)]

    mod raw {
        include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
    }

    pub use raw::root::libcamera::*;
}

pub use bindings::{formats, Orientation, Rectangle, Size, SizeRange, StreamRole};
pub use camera::*;
pub use camera_configuration::*;
pub use camera_manager::*;
pub use color_space::*;
pub use control::Control;
pub use control::*;
pub use control_id::*;
pub use control_info::*;
pub use control_info_map::*;
pub use control_list::*;
pub use control_value::*;
pub use errors::*;
pub use ffi::{CameraConfigurationStatus, FrameBufferPlane, RequestReuseFlag, RequestStatus};
pub use frame_buffer::*;
pub use frame_buffer_allocator::*;
pub use pixel_format::*;
pub use request::*;
pub use sensor_configuration::*;
pub use stream::*;
pub use stream_configuration::*;
pub use stream_formats::*;

pub fn disable_logging() {
    ffi::logSetTarget(bindings::LoggingTarget::LoggingTargetNone);
}

/// A terrible hack borrowed from
/// https://github.com/raspberrypi/rpicam-apps/blob/a2b156fc7607ecd2de8d389767924ef9f66588cd/core/rpicam_app.hpp#L90
pub fn pixel_format_bit_depth(format: PixelFormat) -> usize {
    let name = format.to_string();
    if name.contains("8") {
        return 8;
    }

    if name.contains("10") {
        return 10;
    }

    if name.contains("12") {
        return 12;
    }

    16
}
