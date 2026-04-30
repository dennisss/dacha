use std::collections::HashMap;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor::sync::AsyncMutex;
use executor::channel;
use executor_graph::*;
use executor::{lock, lock_async};
use media_camera_proto::media::camera::*;
use v4l2::Controllable;

use crate::frame::*;
use crate::v4l2::frame_data::*;
use crate::v4l2::controls::*;

/*

Generally how would we use this:

- Find all the V4L2 devices
    - Create one capture op for each of them
        - This will figure out the properties
        - Scan through and find preferred formats.
        -
*/

/*

Read only stufF:
- usb info
- v4l2 capabilities

Writeable stuff:
    - camera:v4l2:[i]:format
    - camera:v4l2:[i]:frame_rate
    - camera:v4l2:[i]:control:[id]

*/

/// Number of buffers used to capture frames from the camera.
///
/// Needs to be at least 2 so that data can be taken from the camera while the
/// next frame is being written into another buffer.
const NUM_FRAME_CAMERA_BUFFERS: usize = 4;

/// NOTE: One op instance can only have one concurrent execution.
pub struct V4L2CaptureOp {
    device: v4l2::Device,

    property_namespace: String,
    properties: AsyncMutex<Properties>,

    format: ImageFormat,
    supported_formats: Vec<PixelFormat>,
}

impl V4L2CaptureOp {
    /*
    General idea is to create and and have all the properties set to defaults.
    - caller can then adjust things
    */

    pub async fn create(device: v4l2::Device, property_namespace: &str) -> Result<Self> {
        let mut inst = Self {
            device,
            property_namespace: property_namespace.to_string(),
            properties: AsyncMutex::default(),
            supported_formats: vec![],
            format: ImageFormat {
                width: 0,
                height: 0,
                frame_rate: 0,
                stride: 0,
                pixel_format: PixelFormat::YUV422,
            },
        };

        // Do initial configuration (this will just estimate the initial values of all
        // the properties).
        inst.configure(HashMap::new()).await?;

        Ok(inst)
    }

    pub async fn properties(&self) -> Result<Properties> {
        lock!(props <= self.properties.lock().await?, {
            Ok(props.clone())
        })
    }

    // TODO: Make this cancel safe
    pub async fn set_properties(&self, states: &PropertiesState) -> Result<()> {

        let controls = self.device.list_controls().await?;

        let id_prefix = format!("{}:control:", &self.property_namespace);

        lock_async!(props <= self.properties.lock().await?, {

            let new_states = set_controls_from_proto(
                &self.device,
                &controls,
                &id_prefix,
                props.state(),
                states
            ).await?;

            props.set_state(new_states);

            Ok(())
        })
    }

    pub fn format(&self) -> ImageFormat {
        self.format.clone()
    }

    pub fn supported_formats(&self) -> Vec<PixelFormat> {
        self.supported_formats.clone()
    }

    /*
    Once running, lot's of this stuff may not be editable.


    If there are any inputs, we should plan to configure for them.
    -

    */

    async fn configure(&mut self, property_overrides: HashMap<String, Property>) -> Result<()> {
        let mut capture_stream = self.device.new_capture_stream()?;

        /*
        dev.list_frame_sizes(pixel_format)
        - Note that in V4L2, the sizes will all be either discrete or there will be one stepwise range.
        */

        let formats = capture_stream.list_formats().await?;

        // let group_id = format!("camera:v4l2:{}", i);

        let mut props = Properties::default();

        let mut group_prop = Property::default(); // props.new_properties();
        group_prop.set_id(self.property_namespace.clone());
        group_prop.spec_mut().set_typ(PropertySpec_Type::GROUP);

        let mut supported_formats = vec![];

        let mut format_prop = group_prop.new_children();
        format_prop.set_id(format!("{}:format", &self.property_namespace));
        format_prop.spec_mut().set_name("Format");
        format_prop.spec_mut().set_typ(PropertySpec_Type::ENUM);

        for format in &formats {
            let v = format_prop.spec_mut().new_values();
            v.set_value_name(format.description.clone());
            v.set_string_value(format.pixelformat.to_string());

            if let Some(f) = PixelFormat::from_fourcc(&format.pixelformat.to_string()) {
                supported_formats.push(f);
            }
        }

        let mut format_prop_state = props.state_mut().new_states();
        format_prop_state.set_id(format_prop.id());

        // Some cameras seem to have metadata capture devices that report capture
        // capabilities get report EINVAL on get_format(). This detects and skips those
        // devices.
        if formats.len() > 0 {
            let mut format = capture_stream.get_format().await?;

            format_prop_state
                .current_value_mut()
                .set_string_value(v4l2::PixelFormat(format.pixelformat()).to_string());

            let mut allowed_frame_sizes = self.device.list_frame_sizes(format.pixelformat()).await?;
            allowed_frame_sizes.sort_by(|a, b| {
                let a = match a {
                    v4l2::FrameSizeRange::Discrete { width, height } => width * height,
                    _ => 0
                };
                let b = match b {
                    v4l2::FrameSizeRange::Discrete { width, height } => width * height,
                    _ => 0
                };

                a.cmp(&b)
            });

            // Default to the largest size (assuming we only support discrete sizes).
            if let v4l2::FrameSizeRange::Discrete { width, height } = allowed_frame_sizes.last().unwrap() {
                format.set_width(*width);
                format.set_height(*height)
            }

            // println!("Allowed frame sizes: {:?}", allowed_frame_sizes);

            // TODO: Make frame size a property.

            // NOTE: We must set this at least once in a device's lifetime, otherwise, it
            // may be in an invalid unconfigured state.
            capture_stream.set_format(format.clone()).await?;

            self.format.width = format.width();
            self.format.height = format.height();

            // TODO: Remove the unwrap here.
            self.format.pixel_format =
                PixelFormat::from_fourcc(&v4l2::PixelFormat(format.pixelformat()).to_string())
                    .unwrap_or(PixelFormat::YUV422);
        }

        // TODO: Add the frame rate as a prop.

        // Note that we don't try getting streaming params if there are no formats as
        // the syscalls tend to fail for metadata only devices.
        if formats.len() > 0 {
            // TODO: Also enumerate supported frame intervals.

            let mut params = capture_stream.get_streaming_params().await?;
            let capture_param = unsafe { &mut params.parm.capture };

            if capture_param.capability & v4l2::V4L2_CAP_TIMEPERFRAME == 0 {
                return Err(err_msg("Device doesn't support setting the frame rate"));
            }

            capture_param.timeperframe.numerator = 1;
            capture_param.timeperframe.denominator = 30;

            capture_stream.set_streaming_params(params).await?;

            self.format.frame_rate = 30;
        }

        let id_prefix = format!("{}:control:", &self.property_namespace);
        controls_to_proto(
            &self.device.list_controls().await?,
            &self.device,
            &id_prefix,
            &mut group_prop,
            props.state_mut()
        ).await?;

        props.add_properties(group_prop);
        self.properties = AsyncMutex::new(props);

        self.supported_formats = supported_formats;

        Ok(())
    }

    async fn execute_impl(&self, mut output: OutputStream) -> Result<()> {
        let mut capture_stream = self.device.new_capture_stream()?;

        // Make memory buffers.
        let (mut capture_stream, capture_buffers) = capture_stream
            .configure_mmap(NUM_FRAME_CAMERA_BUFFERS)
            .await?;

        // TODO: Verify that attempting to dequeue a capture buffer fails until it has
        // data?
        for buf in capture_buffers {
            capture_stream.enqueue_buffer(buf).await?;
        }

        capture_stream.turn_on().await?;

        let capture_stream = Arc::new(capture_stream);

        let mut bundle = TaskResultBundle::new();

        let (sender, receiver) = channel::unbounded();
        bundle.add(
            "Enqueue",
            Self::enqueue_thread(capture_stream.clone(), receiver),
        );
        bundle.add(
            "Dequeue",
            Self::dequeue_thread(capture_stream.clone(), sender, self.format.clone(), output),
        );

        bundle.join().await?;

        capture_stream.turn_off().await?;

        eprintln!("Camera closed!");

        Ok(())
    }

    async fn enqueue_thread(
        capture_stream: Arc<v4l2::Stream<v4l2::MMAPBuffer>>,
        receiver: channel::Receiver<v4l2::MMAPBuffer>,
    ) -> Result<()> {
        while let Ok(buf) = receiver.recv().await {
            capture_stream.enqueue_buffer(buf).await?;
        }

        Ok(())
    }

    async fn dequeue_thread(
        capture_stream: Arc<v4l2::Stream<v4l2::MMAPBuffer>>,
        returner: channel::Sender<v4l2::MMAPBuffer>,
        format: ImageFormat,
        mut output: OutputStream,
    ) -> Result<()> {
        loop {
            let buf = capture_stream.dequeue_buffer().await?;

            let frame = ImageFrame {
                sequence: buf.sequence(),
                monotonic_timestamp: buf
                    .monotonic_timestamp()
                    .ok_or_else(|| err_msg("Frame missing timestamp"))?,
                data: Arc::new(V4L2ImageFrameData {
                    buf: Some(buf),
                    returner: returner.clone(),
                }),
                init_data: vec![],
                format: format.clone(),
            };

            output.write(Box::new(frame)).await?;
        }

        output.close().await;

        Ok(())
    }
}

#[async_trait]
impl Operation for V4L2CaptureOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "V4L2Capture".to_string(),
            num_inputs: 0,
            num_outputs: 1,
        }
    }

    async fn execute(
        &self,
        inputs: Vec<InputStream>,
        mut outputs: Vec<OutputStream>,
    ) -> Result<()> {
        self.execute_impl(outputs.pop().unwrap()).await
    }
}
