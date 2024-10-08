use std::time::{Duration, Instant};
use std::{collections::HashMap, sync::Arc};

use common::async_std::task::current;
use common::bytes::Bytes;
use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::channel::error::SendError;
use executor::channel::spsc;
use executor::lock;
use executor::lock_async;
use executor::sync::AsyncMutex;
use executor_graph::{Graph, OutputKey};
use media_camera_proto::media::camera::*;
use parsing::ascii::AsciiString;
use video::h264::NALUnitHeader;
use video::h264::NALUnitType;

use crate::frame::*;
use crate::frame_buffer_op::{ImageFrameBufferOp, ImageFrameSubscribers};
use crate::h264_buffer_op::H264BufferOp;
use crate::libcamera_op::LibcameraOp;
use crate::v4l2::{V4L2CaptureOp, V4L2EncoderOp, V4L2EncoderOptions};

pub enum CameraEntry {
    USB(usb::DeviceEntry),
    Libcamera(libcamera::AvailableCamera),
}

impl CameraEntry {
    pub async fn name(&self) -> Result<String> {
        Ok(match self {
            CameraEntry::USB(usb_entry) => {
                let product_name = usb_entry
                    .product()
                    .await?
                    .unwrap_or_else(|| "Unknown USB Camera".into());

                format!("[V4L2/USB] {}", product_name)
            }
            CameraEntry::Libcamera(camera) => {
                let model = camera
                    .properties()
                    .get(libcamera::properties::Model2)
                    .unwrap_or("Unknown Model".to_string());

                format!("[libcamera] {}", model)
            }
        })
    }

    pub fn id(&self) -> String {
        match self {
            CameraEntry::USB(device) => {
                let id = device.sysfs_dir().as_str().to_string();
                format!("usb:{}", id)
            }
            CameraEntry::Libcamera(camera) => {
                let id = camera.id();
                format!("libcamera:{}", id)
            }
        }
    }
}

pub struct CameraSubscriber {
    camera_id: String,
    shared: Arc<Shared>,
    receiver: spsc::Receiver<ImageFrame>,
}

impl CameraSubscriber {
    /// If this fails, then that means that the camera abruptly failed for some
    /// reason.
    pub async fn recv(&mut self) -> Result<ImageFrame> {
        let data = self.receiver.recv().await?;
        Ok(data)
    }

    pub fn try_recv(&mut self) -> Option<Result<ImageFrame>> {
        self.receiver
            .try_recv()
            .map(|r| r.map_err(|e| Error::from(e)))
    }

    pub async fn format(&self) -> Result<ImageFormat> {
        let state = self.shared.state.lock().await?.read_exclusive();

        let entry = state
            .cameras
            .get(&self.camera_id)
            .ok_or_else(|| err_msg("Missing camera"))?;

        Ok(entry.format.clone())
    }

    pub async fn properties(&self) -> Result<Properties> {
        let state = self.shared.state.lock().await?.read_exclusive();

        let entry = state
            .cameras
            .get(&self.camera_id)
            .ok_or_else(|| err_msg("Missing camera"))?;

        Ok(entry.props.clone())
    }
}

/// Manages a set of connected cameras in order to enable multiplexed access to
/// camera data.
///
/// Currently we always try to get a compressed (H.264) video byte stream.
///
/// - By default, all cameras are uninitialized.
/// - When a caller requests access to a camera feed via open_usb_camera,
///   - If this is the first caller for this camera, the camera is newly opened.
///   - Else a subscriber to the existing stream of frames is returned.
/// - When all subscribers for a camera have been dropped, a camera is closed.
///
/// Note that frame data is not buffered and is sent as soon as it is available.
/// If a subscriber is too slow to read frames, it will observe new frames with
/// skipped sequence numbers.
#[derive(Clone)]
pub struct CameraManager {
    shared: Arc<Shared>,
}

struct Shared {
    state: AsyncMutex<State>,
    usb_context: usb::Context,
    libcamera_manager: Arc<libcamera::CameraManager>,
}

#[derive(Default)]
struct State {
    cameras: HashMap<String, OpenCameraEntry>, // TODO: Prehash and use a no-op hasher.
}

struct OpenCameraEntry {
    subscribers: Arc<ImageFrameSubscribers>,
    props: Properties,
    format: ImageFormat,
}

impl CameraManager {
    pub fn create(
        usb_context: usb::Context,
        libcamera_manager: Arc<libcamera::CameraManager>,
    ) -> Result<Self> {
        Ok(Self {
            shared: Arc::new(Shared {
                state: AsyncMutex::default(),
                usb_context,
                libcamera_manager,
            }),
        })
    }

    // TODO: Also add cameras that are already opened and not available for
    // enumeration.
    pub async fn list(&self) -> Result<HashMap<String, CameraEntry>> {
        let mut out = HashMap::new();

        let devices = self.shared.usb_context.enumerate_devices().await?;
        for device in devices {
            // TODO: instead try a full open via the camera manager and have it tell us if
            // it isn't a camera
            let drivers = device.driver_devices().await?;
            let is_camera = drivers
                .iter()
                .find(|d| d.typ == usb::DriverDeviceType::V4L2)
                .is_some();
            if !is_camera {
                continue;
            }

            let id = device.sysfs_dir().as_str().to_string();

            let entry = CameraEntry::USB(device);
            out.insert(entry.id(), entry);
        }

        let cameras = self.shared.libcamera_manager.cameras();
        for camera in cameras {
            let entry = CameraEntry::Libcamera(camera);
            out.insert(entry.id(), entry);
        }

        Ok(out)
    }

    /// CANCEL SAFE
    pub async fn open(&self, entry: CameraEntry) -> Result<CameraSubscriber> {
        let camera_id = entry.id();

        let camera_id2 = camera_id.clone();

        let shared = self.shared.clone();

        let subscribers = executor::spawn(async move {
            lock_async!(state <= shared.state.lock().await?, {
                let subscribers = match state.cameras.get_mut(&camera_id) {
                    Some(camera_entry) => camera_entry.subscribers.clone(),
                    None => {
                        // TODO: Make this more non-blocking.

                        let (graph, output_names, entry) = match entry {
                            CameraEntry::USB(entry) => Self::open_usb_device(entry).await?,
                            CameraEntry::Libcamera(camera) => {
                                Self::open_libcamera_device(camera).await?
                            }
                        };

                        let subscribers = entry.subscribers.clone();
                        state.cameras.insert(camera_id.clone(), entry);

                        executor::spawn(Self::camera_reader_thread(
                            shared.clone(),
                            camera_id,
                            graph,
                            output_names,
                        ));

                        subscribers
                    }
                };

                Ok::<_, Error>(subscribers)
            })
        })
        .join()
        .await?;

        let receiver = subscribers.subscribe().await?;

        Ok(CameraSubscriber {
            camera_id: camera_id2,
            shared: self.shared.clone(),
            receiver,
        })
    }

    async fn open_libcamera_device(
        camera: libcamera::AvailableCamera,
    ) -> Result<(Graph, Vec<String>, OpenCameraEntry)> {
        let mut props = Properties::default();

        let mut graph = Graph::default();

        let capture_op = Arc::new(LibcameraOp::create(camera)?);
        graph.add_node("camera", capture_op.clone(), &[]);

        let encoder_op = Arc::new(
            V4L2EncoderOp::create(V4L2EncoderOptions {
                input_format: capture_op.format(),
                queue_length: 4,
            })
            .await?,
        );
        graph.add_node(
            "encoder",
            encoder_op.clone(),
            &[OutputKey {
                node_name: "camera".to_string(),
                output_index: 0,
            }],
        );

        // TODO: Dedup the rest of this.

        graph.add_node(
            "output:buffer",
            Arc::new(H264BufferOp::new()),
            &[OutputKey {
                node_name: "encoder".to_string(),
                output_index: 0,
            }],
        );

        let (output_op, subscribers) = ImageFrameBufferOp::new();

        let output_name = "output:h264".to_string();

        graph.add_node(
            &output_name,
            Arc::new(output_op),
            &[OutputKey {
                node_name: "output:buffer".to_string(),
                output_index: 0,
            }],
        );

        return Ok((
            graph,
            vec![output_name],
            OpenCameraEntry {
                subscribers,
                props,
                format: encoder_op.output_format(),
            },
        ));
    }

    async fn open_usb_device(
        entry: usb::DeviceEntry,
    ) -> Result<(Graph, Vec<String>, OpenCameraEntry)> {
        let mut props = Properties::default();

        let v4l2_drivers = {
            let mut out = vec![];

            for device in entry.driver_devices().await? {
                if device.typ != usb::DriverDeviceType::V4L2 {
                    continue;
                }

                let num = device
                    .path
                    .as_str()
                    .strip_prefix("/dev/video")
                    .ok_or_else(|| err_msg("Unknown V4L2 path format"))?
                    .parse::<usize>()?;

                out.push((num, device));
            }

            out.sort_by_key(|v| v.0);

            out.into_iter().map(|(_, v)| v).collect::<Vec<_>>()
        };

        // let mut chosen_

        let mut graph = Graph::default();

        let mut chosen_node_name = None;

        for (i, device) in v4l2_drivers.into_iter().enumerate() {
            let mut dev = v4l2::Device::open(&device.path).await?;
            if dev.supports_output_stream() || !dev.supports_capture_stream() {
                continue;
            }

            let group_id = format!("camera:v4l2:{}", i);
            let capture_op = V4L2CaptureOp::create(dev, &group_id).await?;

            props.add_properties(capture_op.properties().clone());

            // Check all supported formats.

            /*
            dev.list_frame_sizes(pixel_format)
            - Note that in V4L2, the sizes will all be either discrete or there will be one stepwise range.
            */

            if capture_op.supported_formats().contains(&PixelFormat::H264) {
                // TODO: Configure the pixel format and image size.

                chosen_node_name = Some((group_id.clone(), capture_op.format()));

                eprintln!("Selecting camera V4L2 device: {}", device.path.as_str());
            }

            graph.add_node(&group_id, Arc::new(capture_op), &[]);
        }

        let (node_name, format) =
            chosen_node_name.ok_or_else(|| err_msg("No output found that provides H264 data"))?;

        graph.add_node(
            "output:buffer",
            Arc::new(H264BufferOp::new()),
            &[OutputKey {
                node_name,
                output_index: 0,
            }],
        );

        let (output_op, subscribers) = ImageFrameBufferOp::new();

        let output_name = "output:h264".to_string();

        graph.add_node(
            &output_name,
            Arc::new(output_op),
            &[OutputKey {
                node_name: "output:buffer".to_string(),
                output_index: 0,
            }],
        );

        Ok((
            graph,
            vec![output_name],
            OpenCameraEntry {
                subscribers,
                props,
                format,
            },
        ))
    }

    async fn camera_reader_thread(
        shared: Arc<Shared>,
        camera_id: String,
        graph: Graph,
        output_names: Vec<String>,
    ) {
        if let Err(e) =
            Self::camera_reader_thread_impl(&shared, &camera_id, graph, output_names).await
        {
            eprintln!("Camera thread failed: {}", e);
        }

        // NOTE: This will drop the subscriber channels for the camera.
        lock!(state <= shared.state.lock().await.unwrap(), {
            state.cameras.remove(&camera_id);
        });
    }

    async fn camera_reader_thread_impl(
        shared: &Shared,
        camera_id: &str,
        graph: Graph,
        output_names: Vec<String>,
    ) -> Result<()> {
        graph.execute(output_names).await?;
        eprintln!("Camera graph done!");
        Ok(())
    }
}
