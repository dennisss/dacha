// TODO: Move this into a more generic crate.

use std::time::{Duration, Instant};
use std::{collections::HashMap, sync::Arc};

use common::bytes::Bytes;
use common::errors::*;
use common::hash::FastHasherBuilder;
use executor::channel::error::SendError;
use executor::channel::spsc;
use executor::lock;
use executor::lock_async;
use executor::sync::AsyncMutex;
use video::h264::NALUnitHeader;
use video::h264::NALUnitType;

use crate::camera_stream;

/// Number of buffers used to capture frames from the camera.
///
/// Needs to be at least 2 so that data can be taken from the camera while the
/// next frame is being written into another buffer.
const NUM_FRAME_CAMERA_BUFFERS: usize = 4;

/// Maximum number of unprocessed frames that one subscriber can have enqueued
/// before we start dropping frames.
const CAMERA_SUBSCRIBER_QUEUE_LENGTH: usize = 8;

/// If no subscriber is pulling frames from the camera for this amount of time,
/// we will
const CAMERA_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

pub struct CameraSubscriber {
    receiver: spsc::Receiver<CameraFrameData>,
}

impl CameraSubscriber {
    /// If this fails, then that means that the camera abruptly failed for some
    /// reason.
    pub async fn recv(&mut self) -> Result<CameraFrameData> {
        let data = self.receiver.recv().await?;
        Ok(data)
    }

    pub fn try_recv(&mut self) -> Option<Result<CameraFrameData>> {
        self.receiver
            .try_recv()
            .map(|r| r.map_err(|e| Error::from(e)))
    }
}

#[derive(Clone)]
pub struct CameraFrameData {
    pub sequence: u32,

    pub monotonic_timestamp: Duration,

    pub data: Bytes,

    pub format: CameraFormat,

    pub init_data: Vec<Bytes>,
}

#[derive(Clone)]
pub struct CameraFormat {
    pub width: u32,
    pub height: u32,
    pub framerate: u32,
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
#[derive(Default)]
pub struct CameraManager {
    shared: Arc<Shared>,
}

#[derive(Default)]
struct Shared {
    state: AsyncMutex<State>,
}

#[derive(Default)]
struct State {
    cameras: HashMap<String, CameraEntry>, // TODO: Prehash and use a no-op hasher.
}

struct CameraEntry {
    subscribers: Vec<spsc::Sender<CameraFrameData>>,
}

impl CameraManager {
    /// CANCEL SAFE
    pub async fn open_usb_camera(&self, entry: usb::DeviceEntry) -> Result<CameraSubscriber> {
        let camera_id = entry.sysfs_dir().as_str().to_string();

        let shared = self.shared.clone();

        let (sender, receiver) = spsc::bounded(CAMERA_SUBSCRIBER_QUEUE_LENGTH);

        executor::spawn(async move {
            lock_async!(state <= shared.state.lock().await?, {
                match state.cameras.get_mut(&camera_id) {
                    Some(camera_entry) => {
                        camera_entry.subscribers.push(sender);
                    }
                    None => {
                        // TODO: Make this more non-blocking.

                        let capture_stream =
                            Self::pick_capture_stream(entry).await?.ok_or_else(|| {
                                err_msg("No suitable capture stream found for camera.")
                            })?;

                        let entry = CameraEntry {
                            subscribers: vec![sender],
                        };
                        state.cameras.insert(camera_id.clone(), entry);

                        executor::spawn(Self::camera_reader_thread(
                            shared.clone(),
                            camera_id,
                            capture_stream,
                        ));
                    }
                }

                Ok::<_, Error>(())
            })
        })
        .join()
        .await?;

        Ok(CameraSubscriber { receiver })
    }

    async fn pick_capture_stream(
        entry: usb::DeviceEntry,
    ) -> Result<Option<v4l2::UnconfiguredStream>> {
        for device in entry.driver_devices().await? {
            if device.typ != usb::DriverDeviceType::V4L2 {
                continue;
            }

            let mut dev = v4l2::Device::open(&device.path).await?;

            if dev.supports_output_stream() || !dev.supports_capture_stream() {
                continue;
            }

            let stream = dev.new_capture_stream()?;

            // Some cameras seem to have metadata capture devices that report capture
            // capabilities get report EINVAL on get_format(). This detects and skips those
            // devices.
            let formats = stream.list_formats().await?;
            if formats.len() == 0 {
                continue;
            }

            let format = stream.get_format().await?;

            if format.pixelformat() != v4l2::V4L2_PIX_FMT_H264 {
                continue;
            }

            eprintln!("Selecting camera V4L2 device: {}", device.path.as_str());

            return Ok(Some(stream));
        }

        Ok(None)
    }

    async fn camera_reader_thread(
        shared: Arc<Shared>,
        camera_id: String,
        capture_stream: v4l2::UnconfiguredStream,
    ) {
        if let Err(e) = Self::camera_reader_thread_impl(&shared, &camera_id, capture_stream).await {
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
        mut capture_stream: v4l2::UnconfiguredStream,
    ) -> Result<()> {
        // TODO: Pick the highest resolution at which we can encode stuff.
        let format = {
            let mut format = capture_stream.get_format().await?;

            // NOTE: We must set this at least once in a device's lifetime, otherwise, it
            // may be in an invalid unconfigured state.
            capture_stream.set_format(format.clone()).await?;

            // TODO: Also enumerate supported frame intervals.

            let mut params = capture_stream.get_streaming_params().await?;
            let capture_param = unsafe { &mut params.parm.capture };

            if capture_param.capability & v4l2::V4L2_CAP_TIMEPERFRAME == 0 {
                return Err(err_msg("Device doesn't support setting the frame rate"));
            }

            capture_param.timeperframe.numerator = 1;
            capture_param.timeperframe.denominator = 30;

            capture_stream.set_streaming_params(params).await?;

            CameraFormat {
                width: format.width(),
                height: format.height(),
                framerate: 30,
            }
        };

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

        let mut pps = None;
        let mut sps = None;

        let mut last_sent_frame = Instant::now();
        loop {
            let buf = capture_stream.dequeue_buffer().await?;

            let data = Bytes::from(buf.used_memory());

            Self::find_h264_stream_init_data(&data, &mut pps, &mut sps)?;

            let pps = pps
                .clone()
                .ok_or_else(|| err_msg("Camera stream missing PPS"))?;
            let sps = sps
                .clone()
                .ok_or_else(|| err_msg("Camera stream missing SPS"))?;

            let frame = CameraFrameData {
                sequence: buf.sequence(),
                monotonic_timestamp: buf
                    .monotonic_timestamp()
                    .ok_or_else(|| err_msg("Frame missing timestamp"))?,
                data,
                init_data: vec![pps, sps],
                format: format.clone(),
            };

            let now = Instant::now();

            let have_subscribers = lock!(state <= shared.state.lock().await?, {
                let entry = state.cameras.get_mut(camera_id).unwrap();

                let mut i = 0;
                while i < entry.subscribers.len() {
                    if let Err(e) = entry.subscribers[i].try_send(frame.clone()) {
                        if e.error == SendError::ReceiverDropped {
                            entry.subscribers.swap_remove(i);
                            continue;
                        }
                    } else {
                        last_sent_frame = now;
                    }

                    i += 1;
                }

                !entry.subscribers.is_empty()
            });

            // TODO: Exit immediately if !have_subscribers and we get a
            // cancellation/shutdown signal
            if now - last_sent_frame > CAMERA_IDLE_TIMEOUT {
                break;
            }

            capture_stream.enqueue_buffer(buf).await?;
        }

        capture_stream.turn_off().await?;

        eprintln!("Camera closed!");

        Ok(())
    }

    fn find_h264_stream_init_data(
        data: &[u8],
        pps: &mut Option<Bytes>,
        sps: &mut Option<Bytes>,
    ) -> Result<()> {
        if pps.is_some() && sps.is_some() {
            return Ok(());
        }

        let mut iter = video::h264::H264BitStreamIterator::new(data);

        while let Some(nalu) = iter.peek() {
            let (header, rest) = NALUnitHeader::parse(nalu.data())?;
            match header.nal_unit_type {
                NALUnitType::PPS => {
                    // TODO: We'd want to tack the whole NALU
                    *pps = Some(nalu.raw().into());
                }
                NALUnitType::SPS => {
                    *sps = Some(nalu.raw().into());
                }
                _ => {}
            }

            // TODO: Make this simpler.
            nalu.advance();
        }

        Ok(())
    }
}
