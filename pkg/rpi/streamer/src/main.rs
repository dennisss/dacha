#[macro_use]
extern crate macros;

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor::channel;
use executor::sync::Mutex;
use file::{LocalFile, LocalFileOpenOptions, LocalPath};
use sys::MappedMemory;

use rpi_streamer::*;
use video::mp4::MP4Builder;

/*

We are capturing V4L2_PIX_FMT_H264
- "H264 video elementary stream with start codes."

- H264 in RTP: https://www.rfc-editor.org/rfc/rfc6184

    - Byte stream is the "elementary" format (https://wenchy.github.io/blogs/2015-12-11-H.264-stream-structure.html)
    - Wrapper around NAL

*/

// TODO: Switch libcamera to use sys::MappedMemory and sys::Errno.

// TODO: Fix the file API to remap EACCESS

// TODO: ioctl needs to retry EINTR

// TODO: Figure out when Raspberry PI cameras get cropped frames.

/*

Pi Camera 1 is 2592 x 1944 pixels

Size: Size { width: 800, height: 600 }
Pixel Format: YUV420
[31:01:25.565705706] [5627]  INFO Camera camera.cpp:1028 configuring streams: (0) 800x600-YUV420
[31:01:25.566276072] [5628]  INFO RPI raspberrypi.cpp:805 Sensor: /base/soc/i2c0mux/i2c@1/ov5647@36 - Selected sensor format: 1296x972-SGBRG10_1X10 - Selected unicam format: 1296x972-pGAA
Configured!
Stride: 832
Stream: 800x600-YUV420
Stream ID: 7f7c015ae8
Request Controls: ControlList { AeEnable: "true" }
Request: Request(0:C:0/1:0)
Request_Status(1)
Planes: [FrameBufferPlane { fd: 18, offset: 0, length: 499200 }, FrameBufferPlane { fd: 18, offset: 499200, length: 124800 }, FrameBufferPlane { fd: 18, offset: 624000, length: 124800 }]
Timestamp: 111686056696000
Size: 748800
Written!


 */

/*

*/

// TODO: Unsure there is no crop or
// TODO: Disable any dynamic feature like auto-exposure or AWB

// TODO: Set FrameDurationLimits : [i64; 2] where each value should be the frame
// time in microseconds. ^ These should be passed to Camera::start() : Or check
// to see if

// TODO: Ensure that CameraConfiguration::transform is empty (identity)

/*
Properties: ControlList {
    ScalerCropMaximum: "(0, 0)/0x0",
    ColorFilterArrangement: "2",
    PixelArrayActiveAreas: "[ (16, 6)/2592x1944 ]",
    PixelArraySize: "2592x1944",
    Rotation: "0",
    Location: "2",
    UnitCellSize: "1400x1400",
    Model: "ov5647",
}
 */

const CAMERA_QUEUE_LENGTH: usize = 4;
const FRAME_RATE: usize = 24;

const NUM_FRAMES: usize = FRAME_RATE * 20;

/*
Basically goal is to:
- Capture two frames
- Invocation one:
    - Apply a gaussian blur to them (OpenGL kernel?)
        - Also used to copy the frame
- Invocation two:
    - Diff the two frames
    - Sum up the number of pixel values that have changed.
-


General motion detection pipeline:
- Maintain a last_frame which is updated every 10 seconds with a 5 second old frame
- Every new frame is compared about last_frame
- Require 5 consecutive frames to
- Disable motion tracking while home (home is on the wifi network, except bedtime tracking)
    - External trigger if things like door sensors or PIR are triggered
- Will also need auto-switching of IR
- Validation
    - At least 10 seconds of motion per day
    - No more than 1 hour of motion per day
    - Verify frame is similar to frame from a few days ago


*/

pub struct Streamer {
    //
}

impl Streamer {
    /// Input: # of frames
    /// Output: Stream of CameraModuleRequest (raw frames)
    async fn camera_infeed_task(
        mut camera: CameraModule,
        output: channel::Sender<CameraModuleRequest>,
    ) -> Result<()> {
        for _ in 0..NUM_FRAMES {
            let request = camera.request_frame().await?;
            output.send(request).await?;
        }

        Ok(())
    }

    /// Input: Stream of CameraModuleRequest (raw frames)
    /// Output: Stream of ()
    async fn encoder_infeed_task(
        encoder: Arc<H264Encoder>,
        camera_stream_id: u64,
        requests: channel::Receiver<CameraModuleRequest>,
        output: channel::Sender<()>,
    ) -> Result<()> {
        let mut last_frame: Option<Arc<CameraModuleFrame>> = None;

        while let Ok(request) = requests.recv().await {
            let mut frame = request.wait().await?;
            // frame
            //     .buffer_by_id_mut(camera_stream_id)
            //     .unwrap()
            //     .map_memory()?;

            let frame = Arc::new(frame);

            let encoder_data = CameraFrameData {
                frame: frame.clone(),
                stream_id: camera_stream_id,
            };

            /*
            if let Some(last_frame_value) = last_frame.take() {
                let frame_buf = frame
                    .buffer_by_id(camera_stream_id)
                    .unwrap()
                    .used_memory()
                    .unwrap();
                let last_frame_buf = last_frame_value
                    .buffer_by_id(camera_stream_id)
                    .unwrap()
                    .used_memory()
                    .unwrap();

                let mut diff = 0;

                for (a, b) in frame_buf.iter().zip(last_frame_buf.iter()) {
                    diff += ((*a as i64) - (*b as i64)).abs() as u64;
                }

                println!("Diff: {}", diff);
            }

            last_frame = Some(frame);
            */

            // TODO: Check frame buffer status is ok.

            encoder.enqueue_frame(encoder_data).await?;
            output.send(()).await?;
        }

        Ok(())
    }

    /// Input: Stream of H264 video chunks
    /// Output: MP4 file written to disk.
    async fn encoder_outfeed_task(
        encoder: Arc<H264Encoder>,
        mut mp4_builder: MP4Builder,
        inputs: channel::Receiver<()>,
    ) -> Result<()> {
        println!("Start outfeed");

        let mut i = 0;

        while let Ok(()) = inputs.recv().await {
            if i % FRAME_RATE == 0 {
                println!("{} / {}", i, NUM_FRAMES);
            }
            i += 1;

            let capture_buffer = encoder.dequeue_data().await?;

            mp4_builder.append(capture_buffer.used_memory())?;

            encoder.return_buffer(capture_buffer).await?;

            // We assume that we're always able to dequeue both pairs of buffers used for
            // each frame after each frame is done.
            {
                let request = encoder.dequeue_frame().await?;
                drop(request);
            }
        }

        file::write("image.mp4", mp4_builder.finish()?).await?;

        println!("Done outfeed");

        Ok(())
    }
}

async fn record_camera() -> Result<()> {
    let mut camera = CameraModule::create(CameraModuleOptions {
        frame_rate: FRAME_RATE,
        queue_length: CAMERA_QUEUE_LENGTH,
    })?;

    let width = camera.config.stream_config(0).size().width as usize;
    let height = camera.config.stream_config(0).size().height as usize;
    let stride = camera.config.stream_config(0).stride() as usize;
    let stream_id = camera.config.stream_config(0).stream().unwrap().id();

    /*
    TODO: Define a standard struct for tracking the format of frames across operations
    Things I need to know about the camera output:
    - Colorspace
    - Pixel format
    - Width
    - Height
    - Stride
    */

    // TODO: Check the libcamera frame size is the expected size for the (width,
    // height, pixel_format). e.g. there is no vertical padding that we don't
    // expect.

    let mut encoder = Arc::new(
        H264Encoder::create(H264EncoderOptions {
            width,
            height,
            stride,
            framerate: FRAME_RATE,
            queue_length: CAMERA_QUEUE_LENGTH,
        })
        .await?,
    );

    let mp4_builder = MP4Builder::new(width as u32, height as u32, FRAME_RATE as u32)?;

    let mut bundle = TaskResultBundle::new();

    let mut start_time = Instant::now();

    let (request_sender, request_receiver) = channel::unbounded();
    let (encoded_sender, encoded_receiver) = channel::unbounded();

    // Captures frames and sends them out via the 'request' channel.
    bundle.add(
        "CameraInfeed",
        Streamer::camera_infeed_task(camera, request_sender),
    );

    // Feeds frames to the H264 encoder.
    bundle.add(
        "EncoderInfeed",
        Streamer::encoder_infeed_task(encoder.clone(), stream_id, request_receiver, encoded_sender),
    );

    // Pulls encoded data from the H264 encoder and dumbs to disk.
    bundle.add(
        "EncoderOutfeed",
        Streamer::encoder_outfeed_task(encoder.clone(), mp4_builder, encoded_receiver),
    );

    bundle.join().await?;

    let mut end_time = Instant::now();

    println!("Took: {:?}", end_time - start_time);

    println!("Done!");

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    record_camera().await?;

    Ok(())
}
