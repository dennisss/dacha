use std::sync::Arc;

use common::errors::*;
use common::io::Readable;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::AsyncVariable;
use video::mp4::MP4BuilderOptions;
use http_util::internal_server_error;

use crate::frame::PixelFormat;
use crate::camera_manager::{CameraManager, CameraSubscriber};

// TODO: Somewhere we should verify that we are indeed getting frames at 30fps
// and we aren't stalled.
pub async fn respond_with_camera_stream(
    mut subscriber: CameraSubscriber,
) -> Result<http::Response> {

    let format = subscriber.format().await?;

    match format.pixel_format {
        PixelFormat::MJPG => {
            let boundary = "mjpeg-frame-separator".to_string();;

            let typ = format!("multipart/x-mixed-replace;boundary=--{}", boundary);
            let body = MJPGCameraStreamBody {
                subscriber,
                data: vec![],
                boundary
            };

            Ok(http::ResponseBuilder::new()
                .status(http::status_code::OK)
                .header("Content-Type", typ)
                .body(Box::new(body))
                .build()?)
        }
        PixelFormat::H264 => {
            let first_frame = subscriber.recv().await?;

            let mut options = MP4BuilderOptions::default();
            options.fragment = Some(1);
            // Since we may not be the only observers of this camera, we may need to wait
            // for the next iframe.
            options.skip_to_key_frame = true;

            let mut mp4_builder = video::mp4::MP4Builder::new(
                first_frame.format.width,
                first_frame.format.height,
                first_frame.format.frame_rate,
                options,
            )?;

            for data in &first_frame.init_data {
                mp4_builder.append(&data, None, false)?;
            }

            let mime_type = mp4_builder.mime_type()?;

            let body = MP4CameraStreamBody {
                subscriber,
                last_sequence_number: first_frame.sequence,
                mp4_builder,
                data: vec![],
            };

            // TODO: Base the content type on the H264 parameters
            Ok(http::ResponseBuilder::new()
                .status(http::status_code::OK)
                .header("Content-Type", mime_type)
                .body(Box::new(body))
                .build()?)
        }
        _ => {
            return Err(format_err!("Don't know how to stream back format: {:?}", format.pixel_format));
        }
    }
}

/// http::Body which continously sends back an MP4 from a camera.  
struct MP4CameraStreamBody {
    /// Subscriber used to get raw H264 packets from the camera.
    subscriber: CameraSubscriber,

    /// Sequence number of the last frame received from the camera.
    last_sequence_number: u32,

    mp4_builder: video::mp4::MP4Builder,

    /// MP4 data that has been generated but not yet read from the body.
    data: Vec<u8>,
}

#[async_trait]
impl Readable for MP4CameraStreamBody {
    async fn read(&mut self, out: &mut [u8]) -> Result<usize> {
        loop {
            // TODO: If we are able to write >0 bytes, make all operations try_recv and stop
            // once we can't make more progress without blocking.

            if !self.data.is_empty() {
                let n = core::cmp::min(out.len(), self.data.len());
                out[0..n].copy_from_slice(&self.data[0..n]);
                self.data = self.data.split_off(n);
                return Ok(n);
            }

            if let Some(chunk) = self.mp4_builder.consume() {
                self.data = chunk.data;
                continue;
            }

            let frame = self.subscriber.recv().await?;
            if frame.sequence != self.last_sequence_number + 1 {
                return Err(err_msg("Some frames skipped"));
            }

            self.last_sequence_number = frame.sequence;
            self.mp4_builder.append(
                &frame.data.data().unwrap(),
                Some(frame.monotonic_timestamp),
                false,
            )?;
        }
    }
}

#[async_trait]
impl http::Body for MP4CameraStreamBody {
    fn len(&self) -> Option<usize> {
        None
    }

    async fn trailers(&mut self) -> Result<Option<http::Headers>> {
        Ok(None)
    }
}

/// http::Body which streams back MJPEG frames.
///
/// See https://en.wikipedia.org/wiki/Motion_JPEG
struct MJPGCameraStreamBody {
    subscriber: CameraSubscriber,

    /// Pendign data which we haven't yet 
    data: Vec<u8>,
    
    boundary: String,
}

#[async_trait]
impl Readable for MJPGCameraStreamBody {
    async fn read(&mut self, out: &mut [u8]) -> Result<usize> {

        loop {
            if !self.data.is_empty() {
                let n = core::cmp::min(out.len(), self.data.len());
                out[0..n].copy_from_slice(&self.data[0..n]);
                self.data = self.data.split_off(n);
                return Ok(n);
            }

            let frame = self.subscriber.recv().await?;

            self.data.extend_from_slice(format!("\r\n--{}\r\nContent-Type: image/jpeg\r\n\r\n", self.boundary).as_bytes());
            self.data.extend_from_slice(&frame.data.data().unwrap());
        }
    }
}

#[async_trait]
impl http::Body for MJPGCameraStreamBody {
    fn len(&self) -> Option<usize> {
        None
    }

    async fn trailers(&mut self) -> Result<Option<http::Headers>> {
        Ok(None)
    }
}

