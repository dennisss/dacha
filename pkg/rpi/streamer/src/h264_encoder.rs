use std::sync::Arc;

use common::errors::*;
use executor::channel::queue::ConcurrentQueue;

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
        let mut dev = v4l2::Device::open("/dev/video11")?;

        // TODO: Explicitly set the H264 profile?

        let mut output_stream =
            dev.new_stream(v4l2::v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)?;
        {
            let mut format = v4l2::v4l2_format::default();
            format.fmt.pix_mp.width = options.width as u32;
            format.fmt.pix_mp.height = options.height as u32;
            format.fmt.pix_mp.pixelformat = v4l2::V4L2_PIX_FMT_YUV420;
            unsafe { format.fmt.pix_mp.plane_fmt[0].bytesperline = options.stride as u32 };
            format.fmt.pix_mp.field = v4l2::v4l2_field::V4L2_FIELD_ANY.0;
            format.fmt.pix_mp.colorspace = v4l2::v4l2_colorspace::V4L2_COLORSPACE_REC709.0;
            format.fmt.pix_mp.num_planes = 1;

            /*
            let mut format = output_stream.get_format().await?;
            format.fmt.pix_mp.width = 800;
            format.fmt.pix_mp.height = 600;
            format.fmt.pix_mp.pixelformat = v4l2::V4L2_PIX_FMT_YUV420;
            */

            output_stream.set_format(format).await?;

            // Set frame rate
            let mut param = v4l2::v4l2_streamparm::default();
            param.parm.output.timeperframe.numerator = 1;
            param.parm.output.timeperframe.denominator = options.framerate as u32;
            output_stream.set_streaming_params(param).await?;
        }

        let mut capture_stream =
            dev.new_stream(v4l2::v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)?;
        {
            let mut format = v4l2::v4l2_format::default();
            format.fmt.pix_mp.width = options.width as u32;
            format.fmt.pix_mp.height = options.height as u32;
            format.fmt.pix_mp.pixelformat = v4l2::V4L2_PIX_FMT_H264;
            format.fmt.pix_mp.field = v4l2::v4l2_field::V4L2_FIELD_ANY.0;
            format.fmt.pix_mp.colorspace = v4l2::v4l2_colorspace::V4L2_COLORSPACE_DEFAULT.0;
            format.fmt.pix_mp.num_planes = 1;
            unsafe {
                format.fmt.pix_mp.plane_fmt[0].bytesperline = 0;
                format.fmt.pix_mp.plane_fmt[0].sizeimage = 512 << 10;
            }

            /*
            let mut format = capture_stream.get_format().await?;
            println!("Capture format: {}", unsafe {
                // Should be the 'H264' four-CC code
                format.fmt.pix_mp.pixelformat
            });

            format.fmt.pix_mp.width = 800;
            format.fmt.pix_mp.height = 600;
            */

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
