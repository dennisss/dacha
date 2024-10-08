use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::bundle::TaskResultBundle;
use executor::channel;
use executor_graph::*;

use crate::frame::*;

const CAMERA_QUEUE_LENGTH: usize = 4;
const TARGET_FPS: u64 = 30;

const CANDIDATE_OUTPUT_SIZES: &'static [libcamera::Size] = &[
    // NOTE: The size size must have the most number of pixels.
    libcamera::Size {
        width: 1920,
        height: 1080,
    },
    libcamera::Size {
        width: 1280,
        height: 720,
    },
];

#[derive(Debug)]
struct CameraSizeConfig {
    sensor_config: libcamera::SensorConfiguration,
    output_size: libcamera::Size,
    scaler_crop: Option<libcamera::Rectangle>,
}

pub struct CameraModuleOptions {
    pub frame_rate: usize,
    pub queue_length: usize,
}

pub struct LibcameraOp {
    shared: Arc<Shared>,
}

struct Shared {
    camera: libcamera::RunningCamera,

    config: libcamera::CameraConfiguration,

    new_request_sender: channel::Sender<libcamera::NewRequest>,
    new_request_receiver: channel::Receiver<libcamera::NewRequest>,
}

impl LibcameraOp {
    pub fn create(camera: libcamera::AvailableCamera) -> Result<Self> {
        println!("Camera Id: {}", camera.id());

        let camera = camera.acquire()?;
        println!("Camera Acquired!");

        let (camera, size_config) = Self::select_camera_config(camera)?;

        println!("Selected Size: {:#?}", size_config);

        let mut config = camera
            .generate_configuration(&[libcamera::StreamRole::Viewfinder])
            .unwrap();
        assert_eq!(config.stream_configs_len(), 1);

        config.set_sensor_config(Some(size_config.sensor_config));
        config
            .stream_config_mut(0)
            .set_size(size_config.output_size);

        // Only allocate one buffer per stream.
        config
            .stream_config_mut(0)
            .set_buffer_count(CAMERA_QUEUE_LENGTH as u32);

        /*
        TODO

        If doing video then want:
            cfg.colorSpace = libcamera::ColorSpace::Rec709;
        Else if JPEG then
            cfg.colorSpace = libcamera::ColorSpace::Sycc;
        */
        config
            .stream_config_mut(0)
            .set_color_space(Some(unsafe { libcamera::ColorSpace_Rec709 }));

        let mut found_format = false;
        for format in config.stream_config(0).formats().pixel_formats() {
            if format.to_string() == "YUV420" {
                config.stream_config_mut(0).set_pixel_format(format);
                found_format = true;
                break;
            }
        }

        if !found_format {
            return Err(err_msg("Failed to configure camera format"));
        }

        println!(
            "Camera Pixel Format: {:?}",
            config.stream_config(0).pixel_format()
        );

        if config.validate() != libcamera::CameraConfigurationStatus::Valid {
            return Err(err_msg("Failed to validate camera config"));
        }

        let camera = camera.configure(&mut config)?;
        println!("Camera Configured!");

        let mut frame_buffer_allocator = camera.new_frame_buffer_allocator();

        let stream_config = config.stream_config(0);
        let stream = stream_config.stream().unwrap();

        let frame_buffers = frame_buffer_allocator.allocate(stream)?;

        let mut requests = vec![];
        requests.reserve_exact(frame_buffers.len());

        for frame_buffer in frame_buffers {
            // In v4l2 land, we only support using a single plane right now so we need to
            // verify that the planes can be represented as one contiguous plane starting at
            // offset 0 in the dmabuf file.
            {
                if frame_buffer.planes().is_empty() {
                    return Err(err_msg("Expected at least one plane"));
                }

                let mut last_fd = None;
                let mut last_offset = 0;
                for plane in frame_buffer.planes() {
                    if plane.offset != last_offset {
                        return Err(err_msg("Non-contigous planes in frame buffer"));
                    }

                    last_offset += plane.length;

                    if last_fd.unwrap_or(plane.fd) != plane.fd {
                        return Err(err_msg(
                            "All frame buffer planes must have the same file descriptor",
                        ));
                    }

                    if let Some(fd) = last_fd {
                        if fd != plane.fd {
                            return Err(err_msg("All planes must have the same fd"));
                        }
                    }
                }
            }

            let mut request = camera.create_request(0);
            // println!("Request sequence: {}", request.sequence());

            request.add_buffer(frame_buffer)?;
            requests.push(request);
        }

        println!("Camera Controls Available: {:#?}", camera.controls());

        let mut controls = libcamera::ControlList::new();

        if let Some(scaler_crop) = size_config.scaler_crop {
            controls.set(libcamera::controls::ScalerCrop, scaler_crop);
        }

        let frame_duration = (Duration::from_secs(1).as_micros() as i64) / (TARGET_FPS as i64);
        controls.set(
            libcamera::controls::FrameDurationLimits,
            [frame_duration, frame_duration],
        );

        let (sender, receiver) = channel::unbounded();

        for request in requests {
            sender.try_send(request).unwrap();
        }

        Ok(Self {
            shared: Arc::new(Shared {
                camera: camera.start(Some(&controls))?,
                config,
                new_request_sender: sender,
                new_request_receiver: receiver,
            }),
        })
    }

    /// Tries to automatically big a good camera sensor and output resolution.
    ///
    /// - We only consider raw sensor modes that can output in >= TARGET_FPS and
    ///   cover the full FOV of the sensor.
    /// - The raw sensor output is center cropped (in libcamera) to a standard
    ///   16:9 video resolution.
    /// - We pick the best candidate configuration that covers the most pixels
    ///   in the sensor area. Less downscaling is also preferred if multiple
    ///   output sizes have the same FOV.
    ///  
    fn select_camera_config(
        mut camera: libcamera::AcquiredCamera,
    ) -> Result<(libcamera::AcquiredCamera, CameraSizeConfig)> {
        let config = camera
            .generate_configuration(&[libcamera::StreamRole::Raw])
            .ok_or_else(|| err_msg("Failed to generate a default camera config"))?;

        // Second copy of the config that we can mutate while iterating over 'config'
        let mut config_mut = camera
            .generate_configuration(&[libcamera::StreamRole::Raw])
            .ok_or_else(|| err_msg("Failed to generate a default camera config"))?;

        if config.stream_configs_len() != 1 || config_mut.stream_configs_len() != 1 {
            return Err(err_msg(
                "Expected camera config to have one one stream config",
            ));
        }

        let mut candidates = vec![];

        let formats = config.stream_config(0).formats();

        let pixel_formats = formats.pixel_formats();
        if pixel_formats.len() != 1 {
            return Err(err_msg("Expected camera to only have one raw pixel format"));
        }

        let pixel_format = pixel_formats.get(0).unwrap().clone();

        for sensor_output_size in formats.sizes(pixel_format) {
            config_mut
                .stream_config_mut(0)
                .set_pixel_format(pixel_format);
            config_mut.stream_config_mut(0).set_size(sensor_output_size);

            let mut sensor_config = libcamera::SensorConfiguration::default();
            sensor_config.outputSize = sensor_output_size;
            sensor_config.bitDepth = libcamera::pixel_format_bit_depth(pixel_format) as u32;
            config_mut.set_sensor_config(Some(sensor_config.clone()));
            if config_mut.validate() != libcamera::CameraConfigurationStatus::Valid {
                continue;
            }

            let c = camera.configure(&mut config_mut)?;

            println!("Try: {:?}", sensor_output_size);

            let mut good = true;

            if let Some(info) = c.controls().get(libcamera::controls::FrameDurationLimits) {
                let max_fps = 1_000_000.0 / (info.min().get_i64() as f64);
                if max_fps < TARGET_FPS as f64 {
                    println!("=> Too slow: {}", max_fps);
                    good = false;
                }
            }

            if let Some(info) = c.controls().get(libcamera::controls::ScalerCrop) {
                let sensor_area = c
                    .properties()
                    .get(libcamera::properties::PixelArrayActiveAreas)
                    .ok_or_else(|| err_msg("Camera missing PixelArrayActiveAreas property"))?;
                if sensor_area.len() != 1 {
                    return Err(err_msg(
                        "Only sensors with one active pixel area are supported",
                    ));
                }

                let max_rect = info.max().get_rectangle();

                let is_full_fov = max_rect.x == 0
                    && max_rect.y == 0
                    && max_rect.height == sensor_area[0].height
                    && max_rect.width == sensor_area[0].width;

                if !is_full_fov {
                    println!("=> Not full fov: {:?} vs {:?}", max_rect, sensor_area[0]);
                    good = false;
                }
            }

            if good {
                for output_size in CANDIDATE_OUTPUT_SIZES {
                    println!("=> With {:?}", output_size);

                    if output_size.width > sensor_output_size.width
                        && output_size.height > sensor_output_size.height
                    {
                        // Too big to crop.
                        println!("=> Too small");
                        continue;
                    }

                    let mut candidate = CameraSizeConfig {
                        sensor_config: sensor_config,
                        output_size: output_size.clone(),
                        scaler_crop: None,
                    };

                    if output_size.width != sensor_output_size.width
                        || output_size.height != sensor_output_size.height
                    {
                        // Need a crop.

                        let scaler_crop_info =
                            match c.controls().get(libcamera::controls::ScalerCrop) {
                                Some(v) => v,
                                None => {
                                    println!("=> Can't crop");
                                    continue;
                                }
                            };

                        let sensor_area = scaler_crop_info.max().get_rectangle();

                        // We assume that the output size is using uniform (preserves aspect ratio)
                        // integer scaling of the raw sensor.
                        let scale =
                            (sensor_area.height as f64) / (sensor_output_size.height as f64);
                        if (scale as u64) as f64 != scale {
                            println!("=> Non-scalar");
                            continue;
                        }

                        let scale = scale as u32;

                        let rect_width = output_size.width * scale;
                        let rect_height = output_size.height * scale;

                        candidate.scaler_crop = Some(libcamera::Rectangle {
                            x: ((sensor_area.width - rect_width) / 2) as i32,
                            y: ((sensor_area.height - rect_height) / 2) as i32,
                            width: rect_width,
                            height: rect_height,
                        });
                    }

                    let num_output_pixels =
                        (output_size.height as f64) * (output_size.width as f64);
                    let num_input_pixels =
                        (sensor_output_size.height as f64) * (sensor_output_size.width as f64);

                    let max_output_pixels = (CANDIDATE_OUTPUT_SIZES[0].height as f64)
                        * (CANDIDATE_OUTPUT_SIZES[0].width as f64);

                    let score = (num_output_pixels / num_input_pixels)
                        + 0.1 * (num_output_pixels / max_output_pixels);

                    candidates.push((score, candidate));
                }
            }

            camera = c.unconfigure();
        }

        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        let (_, selected_candidate) = candidates
            .pop()
            .ok_or_else(|| err_msg("Couldn't find any good candidate resolutions for sensor"))?;

        Ok((camera, selected_candidate))
    }

    /// Input: # of frames
    /// Output: Stream of CameraModuleRequest (raw frames)
    async fn execute_impl(&self, mut output: OutputStream) -> Result<()> {
        let (enqueued_sender, enqueued_receiver) = channel::spsc::bounded(8);

        let mut bundle = TaskResultBundle::new();
        bundle.add(
            "Enqueue",
            Self::enqueue_thread(self.shared.clone(), enqueued_sender),
        );
        bundle.add(
            "Waiter",
            Self::wait_thread(self.shared.clone(), enqueued_receiver, output),
        );

        bundle.join().await?;

        // TODO: Explicitly unconfigure the camera here.

        Ok(())
    }

    /// When a request buffer becomes available, (re-)enqueues it into camera
    /// for capturing another frame. Once enqueued, requests are passed through
    /// the 'sender' in the order they will be captured.
    async fn enqueue_thread(
        shared: Arc<Shared>,
        mut sender: channel::spsc::Sender<libcamera::PendingRequest>,
    ) -> Result<()> {
        loop {
            let new_request = shared.new_request_receiver.recv().await?;
            let pending_request = new_request.enqueue()?;

            if let Err(_) = sender.send(pending_request).await {
                break;
            }
        }

        Ok(())
    }

    /// Receives requests enqueued by 'enqueue_thread' and waits for them to
    /// finish. On finishing, the frames are written to the output stream.
    /// Eventually once the frame is dropped, it will end up being re-enqueued
    /// by enqueue_thread.
    async fn wait_thread(
        shared: Arc<Shared>,
        mut receiver: channel::spsc::Receiver<libcamera::PendingRequest>,
        mut output: OutputStream,
    ) -> Result<()> {
        let stream_id = shared.config.stream_config(0).stream().unwrap().id();

        loop {
            let pending_request = receiver.recv().await?;
            let completed_request = pending_request.await;

            if completed_request.status() != libcamera::RequestStatus::RequestComplete {
                return Err(format_err!(
                    "Request not successfully completed: {} , {:?}",
                    completed_request.to_string(),
                    completed_request.status()
                ));
            }

            // TODO: Make sure this and the request state are always checked
            // before accessing data.
            /*
            assert_eq!(
                frame_buffer.metadata().status,
                libcamera::FrameStatus::FrameSuccess
            );
            */

            let timestamp = Duration::from_nanos(
                completed_request
                    .metadata()
                    .get(libcamera::controls::SensorTimestamp)
                    .unwrap() as u64,
            );

            let frame = ImageFrame {
                sequence: completed_request.sequence(),
                monotonic_timestamp: timestamp,
                data: Arc::new(LibcameraFrameData {
                    request: Some(completed_request),
                    stream_id,
                    returner: shared.new_request_sender.clone(),
                }),
                format: Self::format_impl(&shared),
                init_data: vec![],
            };

            output.write(Box::new(frame)).await?;
        }

        Ok(())
    }

    pub fn format(&self) -> ImageFormat {
        Self::format_impl(&self.shared)
    }

    fn format_impl(shared: &Shared) -> ImageFormat {
        let width = shared.config.stream_config(0).size().width;
        let height = shared.config.stream_config(0).size().height;
        let stride = shared.config.stream_config(0).stride();
        // let stream_id = camera.config.stream_config(0).stream().unwrap().id();

        // TODO: Have fewer static values here.
        ImageFormat {
            width,
            height,
            frame_rate: TARGET_FPS as u32,
            pixel_format: PixelFormat::YUV420Planar,
            stride,
        }
    }
}

#[async_trait]
impl Operation for LibcameraOp {
    fn signature(&self) -> OperationSignature {
        OperationSignature {
            name: "Libcamera".to_string(),
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

pub struct LibcameraFrameData {
    /// NOTE: Always Some(_) before being dropped.
    request: Option<libcamera::CompletedRequest>,

    stream_id: u64,

    returner: channel::Sender<libcamera::NewRequest>,
}

impl ImageFrameData for LibcameraFrameData {
    fn data<'a>(&'a self) -> Option<&'a [u8]> {
        let request = self.request.as_ref().unwrap();
        let buf = request.buffer_by_id(self.stream_id).unwrap();
        buf.used_memory()
    }

    fn dma_buffer(&self) -> Option<DMABuffer> {
        let request = self.request.as_ref().unwrap();
        let buf = request.buffer_by_id(self.stream_id).unwrap();

        let fd = buf.planes()[0].fd as i32;

        let mut bytes_used = 0;
        let mut length = 0;

        for plane in buf.metadata().planes {
            bytes_used += plane.inner.bytesused as u64;
        }

        for plane in buf.planes() {
            length += plane.length as u64;
        }

        Some(DMABuffer {
            fd,
            offset: 0, // TODO
            bytes_used,
            length,
        })
    }
}

impl Drop for LibcameraFrameData {
    fn drop(&mut self) {
        // Return the frame buffer to the camera.
        // Ignore receiver errors as the CameraModule may have been dropped.
        let _ = self.returner.try_send(self.request.take().unwrap().reuse());
    }
}
