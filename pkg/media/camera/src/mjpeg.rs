
use base_error::*;
use executor::channel;
use common::io::Readable;
use common::bytes::Bytes;
use executor_multitask::BroadcastChannelSubscriber;

// TODO: Merge with the other MJPGCameraStreamBody.

/// http::Body which streams back MJPEG frames.
///
/// See https://en.wikipedia.org/wiki/Motion_JPEG
pub struct MJPGCameraStreamBody {
    subscriber: BroadcastChannelSubscriber<Bytes>,

    /// Pendign data which we haven't yet 
    data: Vec<u8>,
    
    boundary: String,
}

impl MJPGCameraStreamBody {
    pub fn new(subscriber: BroadcastChannelSubscriber<Bytes>) -> Self {

        let boundary = "mjpeg-frame-separator".to_string();;

        Self {
            subscriber,
            data: vec![],
            boundary
        }
    }

    pub fn content_type(&self) -> String {
        format!("multipart/x-mixed-replace;boundary=--{}", self.boundary)
    }
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
            self.data.extend_from_slice(&frame);
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
