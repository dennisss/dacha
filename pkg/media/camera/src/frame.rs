use std::sync::Arc;
use std::time::Duration;

use common::bytes::Bytes;

/// Single 2d image frame from a continous sequence of images (usually from a
/// video / camera feed).
#[derive(Clone)]
pub struct ImageFrame {
    pub sequence: u32,

    pub monotonic_timestamp: Duration,

    pub data: Arc<dyn ImageFrameData>,

    pub format: ImageFormat,

    /// For compressed types like H264 streams, this is shared init data that is
    /// needed to initialize decoder instances.
    pub init_data: Vec<Bytes>,
}

// TODO: Need ops to actually check that they are receiving a supported format.
pub trait ImageFrameData: 'static + Send + Sync {
    /// Returns the raw memory buffer associated with this frame (if it is in
    /// mapped in memory).
    fn data<'a>(&'a self) -> Option<&'a [u8]>;

    fn dma_buffer(&self) -> Option<DMABuffer>;
}

impl ImageFrameData for Bytes {
    fn data<'a>(&'a self) -> Option<&'a [u8]> {
        Some(&self[..])
    }

    fn dma_buffer(&self) -> Option<DMABuffer> {
        None
    }
}

pub struct DMABuffer {
    pub fd: i32,
    pub offset: u64,
    pub length: u64,
    pub bytes_used: u64,
}

#[derive(Clone)]
pub struct ImageFormat {
    pub width: u32,
    pub height: u32,
    pub frame_rate: u32,
    pub pixel_format: PixelFormat,
    pub stride: u32,
}

/*
Formats to support:

    description: "YUYV 4:2:2", flags: 0, pixelformat: "YUYV"
        Bytes: [Y Cb Y Cr]
        => This is the only raw supported output from the USB cameras

    description: "Planar YUV 4:2:0", flags: 0, pixelformat: "YU12"
        Standard, most efficient to work with since Y, U, V planes are all in separate planes

    description: "H.264", flags: 1, pixelformat: "H264"

    description: "JFIF JPEG", flags: 1, pixelformat: "JPEG"
*/

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PixelFormat {
    YUV422,
    YUV420Planar,
    H264,
    JPEG,
    MJPG,
}

impl PixelFormat {
    pub fn from_fourcc(code: &str) -> Option<Self> {
        Some(match code {
            "YUYV" => Self::YUV422,
            "YU12" => Self::YUV420Planar,
            "H264" => Self::H264,
            "JPEG" => Self::JPEG,
            "MJPG" => Self::MJPG,
            _ => return None,
        })
    }
}
