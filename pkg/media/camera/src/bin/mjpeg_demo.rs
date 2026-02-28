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

/*
cargo run --bin mjpeg_demo

cargo run --bin builder -- build //pkg/media/camera:mjpeg_demo --config=//pkg/builder/config:rpi64

scp -r -i ~/.ssh/id_cluster built/pkg/media/camera/mjpeg_demo cluster-user@10.1.1.3:~/



v4l2-ctl -d /dev/v4l-subdev2 --set-ctrl test_pattern=1

v4l2-ctl -d /dev/v4l-subdev2 --set-ctrl exposure=100

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

struct CameraSettings {
    width: u32,
    height: u32,
    subdev_format_code: u32,
    video_pixel_format: u32
}

async fn run_camera_reader(
    sender: channel::Sender<Vec<u8>>,
    cancellation_token: Arc<dyn CancellationToken>
) -> Result<()> {

    let mut video_devs = {
        let mut out = HashMap::new();
        for dev in v4l2::Device::list().await? {
            out.insert(dev.device_num(), dev);
        }

        out
    };

    let mut sub_devs = {
        let mut out = HashMap::new();
        for dev in v4l2::SubDevice::list().await? {
            out.insert(dev.device_num(), dev);
        }

        out
    };

    let mut selected = None;
    for mut media_dev in v4l2::MediaDevice::list().await? {

        let entities = media_dev.list_entities()?;

        for entity in &entities {
            if entity.typ() == v4l2::MediaEntityType::V4L2_SUBDEV_SENSOR {
                let entity_id = entity.id();
                println!("Found camera entity '{}' (id {}) on '{}'", entity.name()?, entity_id, media_dev.path().as_str());
                selected = Some((media_dev, entities, entity_id));
                break;
            }
        }

        if selected.is_some() {
            break;
        }
    }

    let (mut media_dev, entities, camera_id) = selected.ok_or_else(|| err_msg("No camera sensor found"))?;


    let mut entities_by_id = HashMap::new();
    let mut entity_names = HashMap::new();
    for entity in entities {
        entity_names.insert(entity.name()?, entity.id());
        entities_by_id.insert(entity.id(), entity);
    }

    let camera_entity = entities_by_id.get(&camera_id).unwrap();

    let settings = {
        let name = camera_entity.name()?;
        if name.contains("ov9281") {
            CameraSettings {
                width: 1280,
                height: 800,
                subdev_format_code: v4l2::MEDIA_BUS_FMT_Y8_1X8,
                video_pixel_format: v4l2::V4L2_PIX_FMT_GREY

            }
        } else if name.contains("mira220") {
            CameraSettings {
                width: 1600,
                height: 1400,
                subdev_format_code: v4l2::MEDIA_BUS_FMT_SGRBG8_1X8,
                video_pixel_format: v4l2::V4L2_PIX_FMT_SGRBG8
            }
    
        } else {
            return Err(err_msg("Unsupported camera"));
        }
    };


    let csi2_id = *entity_names.get("csi2").ok_or_else(|| err_msg("Failed to find the csi2 entity"))?;
    println!("CSI2 Entity Id: {}", csi2_id);

    let cfe_id = *entity_names.get("rp1-cfe-csi2_ch0").ok_or_else(|| err_msg("Failed to find the RP1 CFE entity"))?;
    println!("CFE ID: {}", cfe_id);

    // Reset all links
    for entity in entities_by_id.values() {
        println!("{}", entity.name()?);

        for link in entity.links() {
            if link.flags().contains(v4l2::MediaLinkFlags::Immutable) {
                continue;
            }

            if link.flags().contains(v4l2::MediaLinkFlags::Enabled) {
                println!("TODO: Disable enabled link!");;

                // TODO: Disable me.
            }
        }
    }

    let camera_source_pad = 0;

    // NOTE: We are assuming that internally the CSI2 device wires up pad 0 to 4
    let csi2_sink_pad = 0;
    let csi2_source_pad = 4;

    // Verify there is a link from the camera to csi2:0
    {
        if camera_entity.links().len() != 1 {
            return Err(err_msg("Expected camera to have one link"));
        }

        let l = &camera_entity.links()[0];

        if l.source().entity_id() != camera_id ||
           l.source().index() != camera_source_pad ||
           l.sink().entity_id() != csi2_id ||
           l.sink().index() != csi2_sink_pad {
            return Err(err_msg("Unexpected camera => csi2 link"));
        }

        if !l.flags().contains(v4l2::MediaLinkFlags::Immutable) || !l.flags().contains(v4l2::MediaLinkFlags::Enabled) {
            return Err(err_msg("Expected camera link to be enabled/immutable"));
        }
    }

    // The CFE device should just have a single pad.
    let cfe_pad = 0;

    println!("Linking csi2 -> cfe...");
    let csi2_entity = entities_by_id.get(&csi2_id).unwrap();
    {
        let mut found = false; 
        for l in csi2_entity.links() {
            if l.source().entity_id() == csi2_id && l.source().index() == csi2_source_pad &&
               l.sink().entity_id() == cfe_id && l.sink().index() == cfe_pad {

                media_dev.enable_link(l)?;
                println!("=> Enabled");

                found = true;
                break;
            }
        }

        if !found {
            return Err(err_msg("Failed to find suitable link"));
        }
    }

    let cfe_entity = entities_by_id.get(&cfe_id).unwrap();

    // TODO: Remove the unwraps.
    let mut camera_subdev = sub_devs.remove(&camera_entity.device_num().unwrap())
        .ok_or_else(|| err_msg("Missing camera subdev"))?;
    let mut csi2_subdev = sub_devs.remove(&csi2_entity.device_num().unwrap())
        .ok_or_else(|| err_msg("Missing csi2 subdev"))?;
    let mut cfe_video = video_devs.remove(&cfe_entity.device_num().unwrap())
        .ok_or_else(|| err_msg("Missing video device"))?;

    println!("Configuring formats...");

    let subdev_format = {
        let mut fmt = v4l2::v4l2_subdev_format::default();
        fmt.format.width = settings.width;
        fmt.format.height = settings.height;
        fmt.format.code = settings.subdev_format_code;
        fmt.format.field = v4l2::v4l2_field::V4L2_FIELD_NONE.0;
        fmt
    };

    camera_subdev.set_format(camera_source_pad, &subdev_format)?;
    csi2_subdev.set_format(csi2_sink_pad, &subdev_format)?;
    csi2_subdev.set_format(csi2_source_pad, &subdev_format)?;

    println!("Capturing a frame...");

    let mut capture_stream = cfe_video.new_capture_stream()?;
    {
        let mut format = capture_stream.get_format().await?;

        format.set_width(settings.width);
        format.set_height(settings.height);
        format.set_pixelformat(settings.video_pixel_format);
        format.set_field(v4l2::v4l2_field::V4L2_FIELD_NONE.0);
        format.set_colorspace(v4l2::v4l2_colorspace::V4L2_COLORSPACE_DEFAULT.0);

        format.set_num_planes(1);
        format.set_plane_format(0, {
            let mut f = v4l2::v4l2_plane_pix_format::default();
            f.bytesperline = settings.width;
            f.sizeimage = 0; 
            f
        });

        capture_stream.set_format(format).await?;
    }

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
            width: settings.width as usize,
            height: settings.height as usize,
            channels: 1,
            data: &data2,
        };

        let mut data = vec![];
        data.reserve_exact(data2.len() / 8);

        encoder.encode_raw(&image, &mut data)?;
        let e = Instant::now();

        let compression_ratio = (data.len() as f32) / (buf.used_memory().len() as f32);

        let _ = sender.try_send(data);

        if i % 10 == 0 {
            println!("Encode in {:?} : Compression {:.2}", e - s, compression_ratio);
        }

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
