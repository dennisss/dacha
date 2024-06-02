use std::ops::{Deref, DerefMut};
use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::channel;

pub struct CameraModuleOptions {
    pub frame_rate: usize,
    pub queue_length: usize,
}

pub struct CameraModule {
    camera: libcamera::RunningCamera,

    /// TODO: Make private.
    pub config: libcamera::CameraConfiguration,

    new_request_sender: channel::Sender<libcamera::NewRequest>,
    new_request_receiver: channel::Receiver<libcamera::NewRequest>,
}

impl CameraModule {
    pub fn create(options: CameraModuleOptions) -> Result<Self> {
        let manager = libcamera::CameraManager::create()?;

        let mut cameras = manager.cameras();
        if cameras.len() != 1 {
            return Err(format_err!(
                "Expected just one camera but found {}",
                cameras.len()
            ));
        }

        let camera = cameras.pop().unwrap();
        println!("Camera Id: {}", camera.id());

        let camera = camera.acquire()?;
        println!("Camera Acquired!");

        let mut config = camera
            .generate_configuration(&[libcamera::StreamRole::Viewfinder])
            .unwrap();
        assert_eq!(config.stream_configs_len(), 1);

        // 2x2 binning on a Camera Module V1 (max 42 FPS)
        config.stream_config_mut(0).set_size(libcamera::Size {
            width: 1296,
            height: 972,
        });

        // Only allocate one buffer per stream.
        config
            .stream_config_mut(0)
            .set_buffer_count(options.queue_length as u32);

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

        println!("Camera Size: {:?}", config.stream_config(0).size());
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

        let frame_duration =
            (Duration::from_secs(1).as_micros() as i64) / (options.frame_rate as i64);
        controls.set(
            libcamera::controls::FrameDurationLimits,
            [frame_duration, frame_duration],
        );

        let (sender, receiver) = channel::unbounded();

        for request in requests {
            sender.try_send(request).unwrap();
        }

        Ok(Self {
            camera: camera.start(Some(&controls))?,
            config,
            new_request_sender: sender,
            new_request_receiver: receiver,
        })
    }

    /// Requests that another frame be captured. This frame will be triggered
    /// after all previously requested frames are done. May block if we hit the
    /// max queue length.
    ///
    /// Returns once the frame has been enqueued without waiting for it be
    /// captured.
    pub async fn request_frame(&mut self) -> Result<CameraModuleRequest> {
        let new_request = self.new_request_receiver.recv().await?;
        let pending_request = new_request.enqueue()?;
        Ok(CameraModuleRequest {
            pending_request,
            returner: self.new_request_sender.clone(),
        })
    }
}

pub struct CameraModuleRequest {
    pending_request: libcamera::PendingRequest,
    returner: channel::Sender<libcamera::NewRequest>,
}

impl CameraModuleRequest {
    pub async fn wait(self) -> Result<CameraModuleFrame> {
        let completed_request = self.pending_request.await;
        if completed_request.status() != libcamera::RequestStatus::RequestComplete {
            return Err(format_err!(
                "Request not successfully completed: {} , {:?}",
                completed_request.to_string(),
                completed_request.status()
            ));
        }

        // TODO: Make sure this and the request state are always checked before
        // accessing data.
        /*
        assert_eq!(
            frame_buffer.metadata().status,
            libcamera::FrameStatus::FrameSuccess
        );
        */

        Ok(CameraModuleFrame {
            request: Some(completed_request),
            returner: self.returner,
        })
    }
}

pub struct CameraModuleFrame {
    /// NOTE: Always Some(_) before being dropped.
    request: Option<libcamera::CompletedRequest>,

    returner: channel::Sender<libcamera::NewRequest>,
}

impl Deref for CameraModuleFrame {
    type Target = libcamera::CompletedRequest;

    fn deref(&self) -> &Self::Target {
        self.request.as_ref().unwrap()
    }
}

impl DerefMut for CameraModuleFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.request.as_mut().unwrap()
    }
}

impl Drop for CameraModuleFrame {
    fn drop(&mut self) {
        // Return the frame buffer to the camera.
        // Ignore receiver errors as the CameraModule may have been dropped.
        let _ = self.returner.try_send(self.request.take().unwrap().reuse());
    }
}
