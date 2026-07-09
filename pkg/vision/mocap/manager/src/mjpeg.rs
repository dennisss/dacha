use std::sync::Arc;

use common::io::Readable;
use common::errors::*;
use mocap_proto::mocap::*;
use http::header::CONTENT_TYPE;

pub async fn create_camera_live_stream(stub: Arc<MocapCameraStub>) -> http::Response {

    // TODO: Limit max rate.

    let req = ReadFramesRequest::default();
    let ctx = rpc::ClientRequestContext::default();

    let mut res_stream = stub.ReadFrames(&ctx, &req).await;

    let body = MJPGCameraStreamBody::new(res_stream);

    http::ResponseBuilder::new()
        .status(http::status_code::OK)
        .header(CONTENT_TYPE, body.content_type())
        .body(Box::new(body))
        .build().unwrap()
}


// TODO: Dedup this.
struct MJPGCameraStreamBody {
    res_stream: rpc::ClientStreamingResponse<ReadFramesResponse>,

    /// Pendign data which we haven't yet 
    data: Vec<u8>,
    
    boundary: String,
}

impl MJPGCameraStreamBody {
    pub fn new(res_stream: rpc::ClientStreamingResponse<ReadFramesResponse>) -> Self {

        let boundary = "mjpeg-frame-separator".to_string();

        Self {
            res_stream,
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

            let res = match self.res_stream.recv().await {
                Some(v) => v,
                None => {
                    self.res_stream.finish().await?;
                    return Err(err_msg("Unexpected end to frames stream"))
                }
            };

            self.data.extend_from_slice(format!("\r\n--{}\r\nContent-Type: image/jpeg\r\n\r\n", self.boundary).as_bytes());
            self.data.extend_from_slice(res.mjpeg());
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
