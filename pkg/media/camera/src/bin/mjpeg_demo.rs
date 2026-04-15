#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;

use base_error::*;
use file::LocalPath;
use http::status_code::OK;
use http::header::CONTENT_TYPE;
use executor::channel;
use common::io::Readable;
use executor::cancellation::CancellationToken;
use media_camera::rp1_direct::*;

/*
cargo run --bin mjpeg_demo

cargo run --bin builder -- build //pkg/media/camera:mjpeg_demo --config=//pkg/builder/config:rpi64

scp -r -i ~/.ssh/id_cluster built/pkg/media/camera/mjpeg_demo cluster-user@10.1.1.12:~/

TODO: `sudo apt install v4l-utils`


v4l2-ctl -d /dev/v4l-subdev2 --all

v4l2-ctl -d /dev/v4l-subdev2 --set-ctrl test_pattern=1

v4l2-ctl -d /dev/v4l-subdev2 --set-ctrl exposure=36

v4l2-ctl -d /dev/v4l-subdev2 --set-ctrl analogue_gain=32


v4l2-ctl -d /dev/v4l-subdev2 --set-ctrl test_pattern=100

*/

// TODO: Configure camera controls.
/*
exposure 0x00980911 (int)    : min=1 max=3460 step=1 default=642 value=3460
analogue_gain 0x009e0903 (int)    : min=16 max=255 step=1 default=16 value=125
*/

/*
- Find the media device that has the camera
- Reset/disable all links in that media device
- Verify the camera entity has a link (immutable/enabled) to 'csi2 entity : pad 0'
    - We assume that on the csi2 entity, 'pad 0' is internally wires to 'pad 4'
- Link 'csi2 entity ; pad 4' => 'rp1-cfe-csi2_ch0'

pinctrl 35 oh dh

pinctrl 35 op dl
*/




/// http::Body which streams back MJPEG frames.
///
/// See https://en.wikipedia.org/wiki/Motion_JPEG
struct MJPGCameraStreamBody {
    subscriber: channel::Receiver<Vec<u8>>,

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


/*


#[derive(Args)]
struct Args {
    port: u16,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;


}

*/

pub struct HttpHandler {
    frame_subscriber: channel::Receiver<Vec<u8>>
}

const INDEX_HTML: &'static str = r#"
<!doctype html>
<html>
    <head>

    </head>
    <body>
        hello!

        <img src="/camera" />
    </body>
</html>
"#;

#[async_trait]
impl http::ServerHandler for HttpHandler {
    async fn handle_request<'a>(
        &self,
        req: http::Request,
        ctx: http::ServerRequestContext<'a>,
    ) -> http::Response {


        match req.head.uri.path.as_str() {

            "/" => {
                http::ResponseBuilder::new()
                    .status(OK)
                    .header(CONTENT_TYPE, "text/html")
                    .body(http::BodyFromData(INDEX_HTML.as_bytes()))
                    .build()
                    .unwrap()
            }

            "/camera" => {

                let boundary = "mjpeg-frame-separator".to_string();;

                let typ = format!("multipart/x-mixed-replace;boundary=--{}", boundary);
                let body = MJPGCameraStreamBody {
                    subscriber: self.frame_subscriber.clone(),
                    data: vec![],
                    boundary
                };

                http::ResponseBuilder::new()
                    .status(http::status_code::OK)
                    .header(CONTENT_TYPE, typ)
                    .body(Box::new(body))
                    .build().unwrap()
            }

            rpc_util::PROFILEZ_PATH => {
                rpc_util::ProfilezRequestHandler::default().handle_request(req, ctx).await
            }

            _ => {
                http_util::not_found()        
            }
        }

        
    }
}



async fn run_camera_reader(
    sender: channel::Sender<Vec<u8>>,
    cancellation_token: Arc<dyn CancellationToken>
) -> Result<()> {

    let cam = RP1DirectCamera::open().await?;

    println!("Capturing a frame...");

    let mut capture_stream = cam.capture_stream;

    let (mut capture_stream, capture_buffers) = capture_stream.configure_mmap(4).await?;
    println!("Config mmap!");

    for buf in capture_buffers {
        capture_stream.enqueue_buffer(buf).await?;

        println!("Enqueue!");
    }

    println!("On");
    capture_stream.turn_on().await?;
    println!("Done!");

    use image::format::jpeg::encoder::JPEGEncoder;

    let mut encoder = JPEGEncoder::new(80);
    encoder.use_default_tables();

    let mut i = 0;

    while !cancellation_token.is_cancelled().await {

        let buf = capture_stream.dequeue_buffer().await?;


        let s = Instant::now();

        // Copy out of the mmap'ed dma buffer.
        //
        // Directly accessing the buffer is very slow since it is in non-cacheable memory usually.
        //
        // TODO: Look into other approaches like DMA_BUF_IOCTL_SYNC
        let data2 = buf.used_memory().to_vec();

        let image = image::ImageRef {
            width: cam.width as usize,
            height: cam.height as usize,
            channels: 1,
            data: &data2,
        };

        let mut data = vec![];
        data.reserve_exact(data2.len() / 8);

        encoder.encode_raw(&image, &mut data)?;
        let e = Instant::now();

        let compression_ratio = (data.len() as f32) / (buf.used_memory().len() as f32);

        let _ = sender.try_send(data);

        // if i % 10 == 0 {
        println!("Encode {} in {:?} : Compression {:.2}", i, e - s, compression_ratio);
        // }

        i += 1;


        capture_stream.enqueue_buffer(buf).await?;
    }

    capture_stream.turn_off().await?;
    
    Ok(())
}


#[executor_main]
async fn main() -> Result<()> {


    let root_resource = executor_multitask::RootResource::new();

    let (sender, receiver) = channel::bounded(1);

    root_resource.spawn("CameraReader", |token| run_camera_reader(sender, token)).await;

    {
        let handler = HttpHandler { frame_subscriber: receiver };

        let mut options = http::ServerOptions::default();
        options.port = Some(8001);

        let server = http::Server::new(handler, options);
        root_resource.register_dependency(Arc::new(server.start())).await;
    }


    root_resource.wait().await
}
