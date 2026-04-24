#[macro_use]
extern crate sys;
#[macro_use]
extern crate base_util;
#[macro_use]
extern crate async_trait;
extern crate alloc;

mod bindings {
    //! Bindgen produced bindings.

    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    #![allow(unused)]

    const fn v4l2_fourcc(a: char, b: char, c: char, d: char) -> u32 {
        (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
    }

    const fn v4l2_fourcc_be(a: char, b: char, c: char, d: char) -> u32 {
        v4l2_fourcc(a, b, c, d) | (1 << 31)
    }

    pub fn v4l2_type_is_multiplane(typ: v4l2_buf_type) -> bool {
        typ == v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE
            || typ == v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE
    }

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

    include!(concat!(env!("OUT_DIR"), "/formats.rs"));
}

mod io {
    // From "linux/videodev2.h"
    // By replacing `#define ([A-Z_]+)\s+_IO([A-Z]+)\(` with `io\L$2!(\L$1, `
    // TODO: Auto-generate these from the headers.

    use super::bindings::*;
    use sys::{c_int, c_uint};

    ior!(vidioc_querycap, b'V', 0, v4l2_capability);
    iowr!(vidioc_enum_fmt, b'V', 2, v4l2_fmtdesc);
    iowr!(vidioc_g_fmt, b'V', 4, v4l2_format);
    iowr!(vidioc_s_fmt, b'V', 5, v4l2_format);
    iowr!(vidioc_reqbufs, b'V', 8, v4l2_requestbuffers);
    iowr!(vidioc_querybuf, b'V', 9, v4l2_buffer);
    ior!(vidioc_g_fbuf, b'V', 10, v4l2_framebuffer);
    iow!(vidioc_s_fbuf, b'V', 11, v4l2_framebuffer);
    iow!(vidioc_overlay, b'V', 14, c_int);
    iowr!(vidioc_qbuf, b'V', 15, v4l2_buffer);
    iowr!(vidioc_expbuf, b'V', 16, v4l2_exportbuffer);
    iowr!(vidioc_dqbuf, b'V', 17, v4l2_buffer);
    iow!(vidioc_streamon, b'V', 18, c_uint);
    iow!(vidioc_streamoff, b'V', 19, c_uint);
    iowr!(vidioc_g_parm, b'V', 21, v4l2_streamparm);
    iowr!(vidioc_s_parm, b'V', 22, v4l2_streamparm);
    ior!(vidioc_g_std, b'V', 23, v4l2_std_id);
    iow!(vidioc_s_std, b'V', 24, v4l2_std_id);
    iowr!(vidioc_enumstd, b'V', 25, v4l2_standard);
    iowr!(vidioc_enuminput, b'V', 26, v4l2_input);
    iowr!(vidioc_g_ctrl, b'V', 27, v4l2_control);
    iowr!(vidioc_s_ctrl, b'V', 28, v4l2_control);
    iowr!(vidioc_g_tuner, b'V', 29, v4l2_tuner);
    iow!(vidioc_s_tuner, b'V', 30, v4l2_tuner);
    ior!(vidioc_g_audio, b'V', 33, v4l2_audio);
    iow!(vidioc_s_audio, b'V', 34, v4l2_audio);
    iowr!(vidioc_queryctrl, b'V', 36, v4l2_queryctrl);
    iowr!(vidioc_querymenu, b'V', 37, v4l2_querymenu);
    ior!(vidioc_g_input, b'V', 38, c_int);
    iowr!(vidioc_s_input, b'V', 39, c_int);
    iowr!(vidioc_g_edid, b'V', 40, v4l2_edid);
    iowr!(vidioc_s_edid, b'V', 41, v4l2_edid);
    ior!(vidioc_g_output, b'V', 46, c_int);
    iowr!(vidioc_s_output, b'V', 47, c_int);
    iowr!(vidioc_enumoutput, b'V', 48, v4l2_output);
    ior!(vidioc_g_audout, b'V', 49, v4l2_audioout);
    iow!(vidioc_s_audout, b'V', 50, v4l2_audioout);
    iowr!(vidioc_g_modulator, b'V', 54, v4l2_modulator);
    iow!(vidioc_s_modulator, b'V', 55, v4l2_modulator);
    iowr!(vidioc_g_frequency, b'V', 56, v4l2_frequency);
    iow!(vidioc_s_frequency, b'V', 57, v4l2_frequency);
    iowr!(vidioc_cropcap, b'V', 58, v4l2_cropcap);
    iowr!(vidioc_g_crop, b'V', 59, v4l2_crop);
    iow!(vidioc_s_crop, b'V', 60, v4l2_crop);
    ior!(vidioc_g_jpegcomp, b'V', 61, v4l2_jpegcompression);
    iow!(vidioc_s_jpegcomp, b'V', 62, v4l2_jpegcompression);
    ior!(vidioc_querystd, b'V', 63, v4l2_std_id);
    iowr!(vidioc_try_fmt, b'V', 64, v4l2_format);
    iowr!(vidioc_enumaudio, b'V', 65, v4l2_audio);
    iowr!(vidioc_enumaudout, b'V', 66, v4l2_audioout);
    ior!(vidioc_g_priority, b'V', 67, u32);
    iow!(vidioc_s_priority, b'V', 68, u32);
    iowr!(vidioc_g_sliced_vbi_cap, b'V', 69, v4l2_sliced_vbi_cap);
    iowr!(vidioc_g_ext_ctrls, b'V', 71, v4l2_ext_controls);
    iowr!(vidioc_s_ext_ctrls, b'V', 72, v4l2_ext_controls);
    iowr!(vidioc_try_ext_ctrls, b'V', 73, v4l2_ext_controls);
    iowr!(vidioc_enum_framesizes, b'V', 74, v4l2_frmsizeenum);
    iowr!(vidioc_enum_frameintervals, b'V', 75, v4l2_frmivalenum);
    ior!(vidioc_g_enc_index, b'V', 76, v4l2_enc_idx);
    iowr!(vidioc_encoder_cmd, b'V', 77, v4l2_encoder_cmd);
    iowr!(vidioc_try_encoder_cmd, b'V', 78, v4l2_encoder_cmd);

    ior!(vidioc_dqevent, b'V', 89, v4l2_event);
    iow!(vidioc_subscribe_event, b'V', 90, v4l2_event_subscription);
    iow!(vidioc_unsubscribe_event, b'V', 91, v4l2_event_subscription);

    // linux/media.h
    iowr!(media_ioc_device_info,   b'|', 0x00, media_device_info);
    iowr!(media_ioc_enum_entities, b'|', 0x01, media_entity_desc);
    iowr!(media_ioc_enum_links,    b'|', 0x02, media_links_enum);
    iowr!(media_ioc_setup_link,    b'|', 0x03, media_link_desc);
    iowr!(media_ioc_g_topology,    b'|', 0x04, media_v2_topology);
    ior!(media_ioc_request_alloc,  b'|', 0x05, c_int);

    // /usr/include/linux/v4l2-subdev.h
    ior!(vidioc_subdev_querycap,            b'V',  0, v4l2_subdev_capability);
    iowr!(vidioc_subdev_g_fmt,              b'V',  4, v4l2_subdev_format);
    iowr!(vidioc_subdev_s_fmt,              b'V',  5, v4l2_subdev_format);
    iowr!(vidioc_subdev_g_frame_interval,   b'V', 21, v4l2_subdev_frame_interval);
    iowr!(vidioc_subdev_s_frame_interval,   b'V', 22, v4l2_subdev_frame_interval);
    iowr!(vidioc_subdev_enum_mbus_code,     b'V',  2, v4l2_subdev_mbus_code_enum);
    iowr!(vidioc_subdev_enum_frame_size,    b'V', 74, v4l2_subdev_frame_size_enum);
    iowr!(vidioc_subdev_enum_frame_interval,b'V', 75, v4l2_subdev_frame_interval_enum);
    iowr!(vidioc_subdev_g_crop,             b'V', 59, v4l2_subdev_crop);
    iowr!(vidioc_subdev_s_crop,             b'V', 60, v4l2_subdev_crop);
    iowr!(vidioc_subdev_g_selection,        b'V', 61, v4l2_subdev_selection);
    iowr!(vidioc_subdev_s_selection,        b'V', 62, v4l2_subdev_selection);
}

mod buffer;
mod control;
mod device;
mod format;
mod stream;
mod sub_device;
mod media;
mod control_helpers;
mod event;

pub use bindings::*;
pub use buffer::*;
pub use control::*;
pub use device::*;
pub use format::*;
pub use stream::*;
pub use sub_device::*;
pub use media::*;
pub use event::*;