use std::sync::Arc;

use common::errors::*;
use common::io::Readable;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::AsyncVariable;
use video::mp4::MP4BuilderOptions;

use crate::camera_manager::{CameraManager, CameraSubscriber};

pub async fn respond_with_any_camera_stream(camera_manager: &CameraManager) -> http::Response {
    match respond_with_any_camera_stream_impl(camera_manager).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}", e);
            http::ResponseBuilder::new()
                .status(http::status_code::INTERNAL_SERVER_ERROR)
                .body(http::EmptyBody())
                .build()
                .unwrap()
        }
    }
}

async fn respond_with_any_camera_stream_impl(
    camera_manager: &CameraManager,
) -> Result<http::Response> {
    let devices = camera_manager.list().await?;

    let (_, camera_entry) = devices
        .into_iter()
        .collect::<Vec<_>>()
        .pop()
        .ok_or_else(|| err_msg("No camera found"))?;

    // TODO: Somewhere we should verify that we are indeed getting frames at 30fps
    // and we aren't stalled.

    let subscriber = camera_manager.open(camera_entry).await?;

    respond_with_camera_stream(subscriber).await
}

pub async fn respond_with_camera_stream(
    mut subscriber: CameraSubscriber,
) -> Result<http::Response> {
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

    let body = CameraStreamBody {
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

/// http::Body which continously sends back an MP4 from a camera.  
struct CameraStreamBody {
    /// Subscriber used to get raw H264 packets from the camera.
    subscriber: CameraSubscriber,

    /// Sequence number of the last frame received from the camera.
    last_sequence_number: u32,

    mp4_builder: video::mp4::MP4Builder,

    /// MP4 data that has been generated but not yet read from the body.
    data: Vec<u8>,
}

#[async_trait]
impl Readable for CameraStreamBody {
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
impl http::Body for CameraStreamBody {
    fn len(&self) -> Option<usize> {
        None
    }

    async fn trailers(&mut self) -> Result<Option<http::Headers>> {
        Ok(None)
    }
}
