use std::sync::Arc;

use common::errors::*;
use executor::channel::queue::ConcurrentQueue;
use v4l2::v4l2_plane_pix_format;

use crate::camera::CameraModuleFrame;

/// TODO: Move to another file.
pub struct CameraFrameData {
    // TODO: Make this stuff private
    pub frame: Arc<CameraModuleFrame>,
    pub stream_id: u64,
}

impl v4l2::DMABufferData for CameraFrameData {
    fn as_raw_fd(&self) -> i32 {
        let buf = self.frame.buffer_by_id(self.stream_id).unwrap();
        buf.planes()[0].fd as i32
    }

    fn bytes_used(&self) -> usize {
        let buf = self.frame.buffer_by_id(self.stream_id).unwrap();

        let mut total = 0;
        for plane in buf.metadata().planes {
            total += plane.inner.bytesused;
        }

        total as usize
    }

    fn length(&self) -> usize {
        let buf = self.frame.buffer_by_id(self.stream_id).unwrap();

        let mut total = 0;
        for plane in buf.planes() {
            total += plane.length;
        }

        total as usize
    }
}

pub struct H264EncoderOptions {
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub framerate: usize,
    pub queue_length: usize,
}

/*
Info on the Raspberry Pi 4 encoder:
    Device: /dev/video11
    Driver: bcm2835-codec
    Card: bcm2835-codec-encode
    Bus Info: platform:bcm2835-codec

*/

pub struct H264Encoder {
    device: v4l2::Device,

    output_stream: v4l2::Stream<v4l2::DMABuffer<CameraFrameData>>,
    output_buffers: ConcurrentQueue<v4l2::DMABuffer<CameraFrameData>>,

    /// Stream on which the hardware encoder returns encoded H264 data.
    /// Buffers are kept enqueued on this stream while the user isn't reading
    /// from them.
    capture_stream: v4l2::Stream<v4l2::MMAPBuffer>,
}

impl H264Encoder {
    /// Creates and starts up an H264 encoder.
    pub async fn create(options: H264EncoderOptions) -> Result<Self> {
        // TODO: When we eventually do enumeration of these, we need to skip devices
        // that are already opened by other parts of the application since we will get
        // access errors with opening them twice (but need to verify).

        let mut dev = {
            let mut found_device = None;

            let mut devices = v4l2::Device::list().await?;

            for device in devices {
                if !device.is_m2m() {
                    continue;
                }

                let formats = device.new_capture_stream()?.list_formats().await?;
                for format in formats {
                    if format.pixelformat.to_string() == "H264" {
                        if found_device.is_some() {
                            return Err(err_msg("Found multiple H264 encoding devices"));
                        }

                        found_device = Some(device);
                        break;
                    }
                }
            }

            found_device.ok_or_else(|| err_msg("Couldn't find a V4L2 H264 encoding device"))?
        };

        // TODO: Explicitly set the H264 profile?

        let mut output_stream = dev.new_output_stream()?;
        {
            let mut format = output_stream.get_format().await?;
            format.set_width(options.width as u32);
            format.set_height(options.height as u32);
            format.set_pixelformat(v4l2::V4L2_PIX_FMT_YUV420);
            format.set_field(v4l2::v4l2_field::V4L2_FIELD_ANY.0);
            format.set_colorspace(v4l2::v4l2_colorspace::V4L2_COLORSPACE_REC709.0);

            format.set_num_planes(1);
            format.set_plane_format(0, {
                let mut f = v4l2_plane_pix_format::default();
                f.bytesperline = options.stride as u32;
                f.sizeimage = 0;
                f
            });

            output_stream.set_format(format).await?;

            // Set frame rate
            // TODO: Improve the safety of this.
            let mut param = v4l2::v4l2_streamparm::default();
            param.parm.output.timeperframe.numerator = 1;
            param.parm.output.timeperframe.denominator = options.framerate as u32;
            output_stream.set_streaming_params(param).await?;
        }

        let mut capture_stream = dev.new_capture_stream()?;
        {
            let mut format = capture_stream.get_format().await?;

            format.set_width(options.width as u32);
            format.set_height(options.height as u32);
            format.set_pixelformat(v4l2::V4L2_PIX_FMT_H264);
            format.set_field(v4l2::v4l2_field::V4L2_FIELD_ANY.0);
            format.set_colorspace(v4l2::v4l2_colorspace::V4L2_COLORSPACE_DEFAULT.0);

            format.set_num_planes(1);
            format.set_plane_format(0, {
                let mut f = v4l2_plane_pix_format::default();
                f.bytesperline = 0;
                f.sizeimage = 512 << 10; // 512 KiB
                f
            });

            capture_stream.set_format(format).await?;
        }

        // Make memory buffers.

        let (mut output_stream, mut output_buffers) =
            output_stream.configure_dma(options.queue_length).await?;
        let (mut capture_stream, capture_buffers) =
            capture_stream.configure_mmap(options.queue_length).await?;

        // TODO: Verify that attempting to dequeue a capture buffer fails until it has
        // data?
        for buf in capture_buffers {
            capture_stream.enqueue_buffer(buf).await?;
        }

        output_stream.turn_on().await?;
        capture_stream.turn_on().await?;

        Ok(Self {
            device: dev,
            output_stream,
            output_buffers: output_buffers.into(),
            capture_stream,
        })
    }

    pub async fn enqueue_frame(&self, data: CameraFrameData) -> Result<()> {
        let mut output_buffer = self.output_buffers.pop_front().await;
        output_buffer.set_data(data);
        self.output_stream.enqueue_buffer(output_buffer).await?;
        Ok(())
    }

    pub async fn dequeue_frame(&self) -> Result<CameraFrameData> {
        let mut output_buffer = self.output_stream.dequeue_buffer().await?;
        let data = output_buffer.take_data().unwrap();
        self.output_buffers.push_back(output_buffer).await;
        Ok(data)
    }

    pub async fn dequeue_data(&self) -> Result<v4l2::MMAPBuffer> {
        self.capture_stream.dequeue_buffer().await
    }

    pub async fn return_buffer(&self, buffer: v4l2::MMAPBuffer) -> Result<()> {
        self.capture_stream.enqueue_buffer(buffer).await?;
        Ok(())
    }
}
