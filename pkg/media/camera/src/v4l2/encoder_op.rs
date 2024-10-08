use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor::channel;
use executor::channel::queue::ConcurrentQueue;
use executor::channel::spsc;
use executor_graph::*;
use v4l2::v4l2_plane_pix_format;

use crate::frame::*;
use crate::v4l2::frame_data::*;

impl v4l2::DMABufferData for ImageFrame {
    fn as_raw_fd(&self) -> i32 {
        self.data.dma_buffer().unwrap().fd
    }

    fn bytes_used(&self) -> usize {
        self.data.dma_buffer().unwrap().bytes_used as usize
    }

    fn length(&self) -> usize {
        self.data.dma_buffer().unwrap().length as usize
    }
}

pub struct V4L2EncoderOptions {
    pub input_format: ImageFormat,
    pub queue_length: usize,
}

/*
Info on the Raspberry Pi 4 encoder:
    Device: /dev/video11
    Driver: bcm2835-codec
    Card: bcm2835-codec-encode
    Bus Info: platform:bcm2835-codec

*/

pub struct V4L2EncoderOp {
    shared: Arc<Shared>,
}

struct Shared {
    input_format: ImageFormat,

    device: v4l2::Device,

    output_stream: v4l2::Stream<v4l2::DMABuffer<ImageFrame>>,
    output_buffers: ConcurrentQueue<v4l2::DMABuffer<ImageFrame>>,

    /// Stream on which the hardware encoder returns encoded H264 data.
    /// Buffers are kept enqueued on this stream while the user isn't reading
    /// from them.
    capture_stream: v4l2::Stream<v4l2::MMAPBuffer>,

    capture_buffer_sender: channel::Sender<v4l2::MMAPBuffer>,
    capture_buffer_receiver: channel::Receiver<v4l2::MMAPBuffer>,
}

impl V4L2EncoderOp {
    /// Enumerates all available encoders on the system.
    pub async fn list() -> Result<Vec<Self>> {
        let mut devices = v4l2::Device::list().await?;

        for device in devices {
            if !device.is_m2m() {
                continue;
            }

            // TODO
        }

        todo!()
    }

    /// Creates and starts up an H264 encoder.
    pub async fn create(options: V4L2EncoderOptions) -> Result<Self> {
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

                /*
                              - FormatDefinition { description: "JFIF JPEG", flags: 1, pixelformat: "JPEG" }
                - Frame Sizes: [Stepwise { min_width: 32, max_width: 1920, step_width: 2, min_height: 32, max_height: 1920, step_height: 2 }]

                             */

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
            format.set_width(options.input_format.width);
            format.set_height(options.input_format.height);
            format.set_pixelformat(v4l2::V4L2_PIX_FMT_YUV420); // YU12
            format.set_field(v4l2::v4l2_field::V4L2_FIELD_ANY.0);
            format.set_colorspace(v4l2::v4l2_colorspace::V4L2_COLORSPACE_REC709.0);

            format.set_num_planes(1);
            format.set_plane_format(0, {
                let mut f = v4l2_plane_pix_format::default();
                f.bytesperline = options.input_format.stride;
                f.sizeimage = 0;
                f
            });

            output_stream.set_format(format).await?;

            // Set frame rate
            // TODO: Improve the safety of this.
            let mut param = v4l2::v4l2_streamparm::default();
            param.parm.output.timeperframe.numerator = 1;
            param.parm.output.timeperframe.denominator = options.input_format.frame_rate;
            output_stream.set_streaming_params(param).await?;
        }

        let mut capture_stream = dev.new_capture_stream()?;
        {
            let mut format = capture_stream.get_format().await?;

            format.set_width(options.input_format.width);
            format.set_height(options.input_format.height);
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

        output_stream.turn_on().await?;
        capture_stream.turn_on().await?;

        let (capture_buffer_sender, capture_buffer_receiver) = channel::unbounded();

        // TODO: Verify that attempting to dequeue a capture buffer fails until it has
        // data?
        for buf in capture_buffers {
            capture_buffer_sender.try_send(buf).unwrap();
        }

        Ok(Self {
            shared: Arc::new(Shared {
                input_format: options.input_format.clone(),
                device: dev,
                output_stream,
                output_buffers: output_buffers.into(),
                capture_stream,
                capture_buffer_sender,
                capture_buffer_receiver,
            }),
        })
    }

    pub fn output_format(&self) -> ImageFormat {
        let mut output_format = self.shared.input_format.clone();
        output_format.pixel_format = PixelFormat::H264;
        output_format.stride = 0;
        output_format
    }

    async fn execute_impl(&self, mut input: InputStream, output: OutputStream) -> Result<()> {
        let mut bundle = TaskResultBundle::new();

        // Channel used to propagate whether or not the inputs are all consumed to the
        // dequeue task so that the dequeue task can stop once all inputs are consumed.
        let (input_sender, input_receiver) = spsc::bounded(10);

        bundle.add(
            "Enqueue",
            Self::enqueue_task(self.shared.clone(), input, input_sender),
        );
        bundle.add(
            "Dequeue",
            Self::dequeue_task(self.shared.clone(), input_receiver, output),
        );
        bundle.join().await?;
        Ok(())
    }

    /// Reads image frames from the input and enqueues them onto the 'output'
    /// stream of the encoder (so that are the inputs to encoding).
    async fn enqueue_task(
        shared: Arc<Shared>,
        mut input: InputStream,
        mut input_sender: spsc::Sender<()>,
    ) -> Result<()> {
        while let Some(input) = input.read().await? {
            let frame = input
                .downcast_ref::<ImageFrame>()
                .ok_or_else(|| err_msg("Wrong input format"))?;

            let mut output_buffer = shared.output_buffers.pop_front().await;
            output_buffer.set_data(frame.clone());
            shared.output_stream.enqueue_buffer(output_buffer).await?;

            let capture_buffer = shared.capture_buffer_receiver.recv().await?;
            shared.capture_stream.enqueue_buffer(capture_buffer).await?;

            // Notify the dequeue thread that another frame was read.
            input_sender.send(()).await?;
        }

        Ok(())
    }

    async fn dequeue_task(
        shared: Arc<Shared>,
        mut input_receiver: spsc::Receiver<()>,
        mut output: OutputStream,
    ) -> Result<()> {
        loop {
            if let Err(e) = input_receiver.recv().await {
                return Err(GraphStreamError {}.into());
            }

            // TODO: Make sure that this eventually gets cancelled.
            let capture_buffer = shared.capture_stream.dequeue_buffer().await?;

            // Dequeue the corresponding output_stream buffer, de-allocate the frame and let
            // the buffer be re-used for capturing in 'enqueue_task'.
            //
            // We assume that we're always able to dequeue both pairs of buffers used for
            // each frame after each frame is done.
            let input_frame = {
                let mut output_buffer = shared.output_stream.dequeue_buffer().await?;
                let data = output_buffer.take_data().unwrap();
                shared.output_buffers.push_back(output_buffer).await;
                data
            };

            let mut output_format = input_frame.format.clone();
            output_format.pixel_format = PixelFormat::H264;
            output_format.stride = 0;

            let output_frame = ImageFrame {
                sequence: input_frame.sequence,
                monotonic_timestamp: input_frame.monotonic_timestamp,
                data: Arc::new(V4L2ImageFrameData {
                    buf: Some(capture_buffer),
                    returner: shared.capture_buffer_sender.clone(),
                }),
                format: output_format,
                init_data: vec![], // TODO
            };

            drop(input_frame);

            output.write(Box::new(output_frame)).await?;
        }

        output.close().await;

        Ok(())
    }
}

#[async_trait]
impl Operation for V4L2EncoderOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "V4L2Encoder".to_string(),
            num_inputs: 1,
            num_outputs: 1,
        }
    }

    async fn execute(
        &self,
        mut inputs: Vec<InputStream>,
        mut outputs: Vec<OutputStream>,
    ) -> Result<()> {
        self.execute_impl(inputs.pop().unwrap(), outputs.pop().unwrap())
            .await
    }
}
