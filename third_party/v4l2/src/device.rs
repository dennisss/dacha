use std::{collections::HashSet, sync::Arc};

use base_error::*;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::AsyncVariable;
use executor::ExecutorPollingContext;
use file::{LocalFile, LocalFileOpenOptions, LocalPath};
use sys::EpollEvents;
use sys::Errno;

use crate::bindings::*;
use crate::io::*;
use crate::stream::*;

pub struct Device {
    handle: Arc<DeviceHandle>,
    streams: HashSet<v4l2_buf_type>,
}

pub(crate) struct DeviceHandle {
    /// Task used to poll for events on the file.
    polling_task: ChildTask,

    pub shared: Arc<DeviceShared>,
}

pub(crate) struct DeviceShared {
    /// File for this device.
    ///
    /// Thread safety is not well defined for all drivers so we require that all
    /// ioctl commands are performed under a file lock.
    ///
    /// See also https://stackoverflow.com/questions/10217779/how-thread-safe-is-v4l2#:~:text=ioctl()%20is%20not%20one,once%20it%20reaches%20ioctl()
    pub file: AsyncVariable<LocalFile>,

    pub capability: v4l2_capability,
}

impl Device {
    pub async fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let file = file::LocalFile::open_with_options(
            path,
            &LocalFileOpenOptions::new()
                .read(true)
                .write(true)
                .non_blocking(true),
        )?;

        let mut capability = v4l2_capability::default();
        unsafe { vidioc_querycap(file.as_raw_fd(), &mut capability) }?;

        let shared = Arc::new(DeviceShared {
            file: AsyncVariable::new(file),
            capability,
        });

        Ok(Self {
            handle: Arc::new(DeviceHandle {
                polling_task: ChildTask::spawn(Self::polling_thread(shared.clone())),
                shared,
            }),
            streams: HashSet::new(),
        })
    }

    pub async fn print_capabiliites(&self) -> Result<()> {
        let file = self.handle.shared.file.lock().await?.read_exclusive();

        let mut caps = v4l2_capability::default();
        unsafe { vidioc_querycap(file.as_raw_fd(), &mut caps) }?;

        /*
        Important things in caps.device_caps:
        V4L2_CAP_STREAMING - needed to support mmap

        V4L2_CAP_VIDEO_CAPTURE
        V4L2_CAP_VIDEO_CAPTURE_MPLANE

        V4L2_CAP_VIDEO_OUTPUT
        V4L2_CAP_VIDEO_OUTPUT_MPLANE

        V4L2_CAP_VIDEO_M2M ?
        */

        println!("Driver: {}", read_null_terminated_string(&caps.driver)?);
        println!("Card: {}", read_null_terminated_string(&caps.card)?);
        println!("Bus Info: {}", read_null_terminated_string(&caps.bus_info)?);

        /*
            TODO: Also want to find the serial number

            Media Driver Info:
        Driver name      : uvcvideo
        Model            : H264 USB Camera: H264 USB Camer
        Serial           : 2020052801
        Bus info         : usb-0000:0c:00.3-3.2.1.4
        Media version    : 6.5.13
        Hardware revision: 0x00000100 (256)
        Driver version   : 6.5.13


            */

        Ok(())
    }

    pub fn supports_capture_stream(&self) -> bool {
        let caps = self.handle.shared.capability.capabilities;
        caps & (V4L2_CAP_VIDEO_CAPTURE | V4L2_CAP_VIDEO_CAPTURE_MPLANE) != 0
    }

    pub fn new_capture_stream(&mut self) -> Result<UnconfiguredStream> {
        let caps = self.handle.shared.capability.capabilities;

        let typ = {
            if caps & V4L2_CAP_VIDEO_CAPTURE_MPLANE != 0 {
                v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE
            } else {
                v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_CAPTURE
            }
        };

        self.new_stream(typ)
    }

    pub fn supports_output_stream(&self) -> bool {
        let caps = self.handle.shared.capability.capabilities;
        caps & (V4L2_CAP_VIDEO_OUTPUT | V4L2_CAP_VIDEO_OUTPUT_MPLANE) != 0
    }

    pub fn new_output_stream(&mut self) -> Result<UnconfiguredStream> {
        let caps = self.handle.shared.capability.capabilities;

        let typ = {
            if caps & V4L2_CAP_VIDEO_OUTPUT_MPLANE != 0 {
                v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE
            } else {
                v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_OUTPUT
            }
        };

        self.new_stream(typ)
    }

    // NOTE: We don't expose this directly to users to ensure that the other methods
    // that normalize usage of _MPLANE types when available are used.
    fn new_stream(&mut self, typ: v4l2_buf_type) -> Result<UnconfiguredStream> {
        if !self.streams.insert(typ) {
            return Err(format_err!(
                "Already configuring a stream with buffer type {:?}",
                typ
            ));
        }

        Ok(UnconfiguredStream {
            device: self.handle.clone(),
            typ,
        })
    }

    /// TODO: The list can change if we switch inputs/outputs.
    pub async fn list_formats(&self, typ: v4l2_buf_type) -> Result<Vec<FormatDefinition>> {
        let file = self.handle.shared.file.lock().await?.read_exclusive();

        let mut out = vec![];

        loop {
            let mut fmt = v4l2_fmtdesc::default();
            fmt.type_ = typ.0;
            fmt.index = out.len() as u32;

            match unsafe { vidioc_enum_fmt(file.as_raw_fd(), &mut fmt) } {
                Ok(i) => {
                    assert_eq!(i, 0);
                }
                Err(Errno::EINVAL) => break,
                Err(e) => return Err(e.into()),
            };

            out.push(FormatDefinition {
                description: read_null_terminated_string(&fmt.description)?,
                flags: fmt.flags,
                pixelformat: fmt.pixelformat,
            });
        }

        Ok(out)
    }

    pub async fn list_frame_sizes(&self, pixel_format: u32) -> Result<Vec<FrameSizeRange>> {
        let file = self.handle.shared.file.lock().await?.read_exclusive();

        let mut out = vec![];

        loop {
            let mut raw = v4l2_frmsizeenum::default();
            raw.pixel_format = pixel_format;
            raw.index = out.len() as u32;

            match unsafe { vidioc_enum_framesizes(file.as_raw_fd(), &mut raw) } {
                Ok(i) => {
                    assert_eq!(i, 0);
                }
                Err(Errno::EINVAL) => break,
                Err(e) => return Err(e.into()),
            };

            let el = unsafe {
                if raw.type_ == v4l2_frmsizetypes::V4L2_FRMSIZE_TYPE_DISCRETE.0 {
                    FrameSizeRange::Discrete {
                        width: raw.__bindgen_anon_1.discrete.width,
                        height: raw.__bindgen_anon_1.discrete.height,
                    }
                } else if raw.type_ == v4l2_frmsizetypes::V4L2_FRMSIZE_TYPE_CONTINUOUS.0
                    || raw.type_ == v4l2_frmsizetypes::V4L2_FRMSIZE_TYPE_STEPWISE.0
                {
                    FrameSizeRange::Stepwise {
                        min_width: raw.__bindgen_anon_1.stepwise.min_width,
                        max_width: raw.__bindgen_anon_1.stepwise.max_width,
                        step_width: raw.__bindgen_anon_1.stepwise.step_width,
                        min_height: raw.__bindgen_anon_1.stepwise.min_height,
                        max_height: raw.__bindgen_anon_1.stepwise.max_height,
                        step_height: raw.__bindgen_anon_1.stepwise.step_height,
                    }
                } else {
                    return Err(err_msg("Unsupported frame size range type"));
                }
            };

            out.push(el);
        }

        Ok(out)
    }

    // vidioc_enumaudio
    // vidioc_enum_framesizes
    // vidioc_enum_frameintervals
    // vidioc_g_audio

    async fn polling_thread(shared: Arc<DeviceShared>) {
        if let Err(e) = Self::polling_thread_inner(&shared).await {
            eprintln!("V4L2 Polling Error: {}", e);

            // We assume that the users call ioctl, linux will return errors (so we don't
            // need to store this error).
            Self::notify_all(shared.as_ref()).await;
        }
    }

    async fn polling_thread_inner(shared: &DeviceShared) -> Result<()> {
        let mut ctx = {
            let file = shared.file.lock().await?.read_exclusive();
            unsafe {
                ExecutorPollingContext::create_with_raw_fd(file.as_raw_fd(), EpollEvents::EPOLLIN)
                    .await
            }?
        };

        loop {
            let mut events = ctx.wait().await?;

            if events.contains(EpollEvents::EPOLLIN) {
                events = events.remove(EpollEvents::EPOLLIN);
                Self::notify_all(shared).await;
            }

            // EPOLLHUP implies the device was disconnected probably.
            // TODO: Verify this works right.
            if events.contains(EpollEvents::EPOLLHUP) {
                events = events.remove(EpollEvents::EPOLLHUP);
                Self::notify_all(shared).await;
            }

            if events != EpollEvents::empty() {
                // We will get an EPOLLERR until all the streams are turned up.
                // TODO: Get back to a state where we can return these errors.s
                // (Also ensure all Rust side waiters are aware when this happens).
                eprintln!("Unknown poll events received: {:?}", events);
            }
        }
    }

    async fn notify_all(shared: &DeviceShared) {
        let file = match shared.file.lock().await {
            Ok(v) => v,
            Err(_) => return,
        };

        lock!(file <= file, {
            file.notify_all();
        });
    }

    // pub
}

#[derive(Clone, Debug)]
pub enum FrameSizeRange {
    Discrete {
        width: u32,
        height: u32,
    },
    Stepwise {
        min_width: u32,
        max_width: u32,
        step_width: u32,
        min_height: u32,
        max_height: u32,
        step_height: u32,
    },
}

#[derive(Clone, Debug)]
pub struct FormatDefinition {
    pub description: String,
    pub flags: u32,
    pub pixelformat: u32,
}

// TODO: Deduplicate this everywhere.
fn read_null_terminated_string(data: &[u8]) -> Result<String> {
    for i in 0..data.len() {
        if data[i] == 0x00 {
            return Ok(std::str::from_utf8(&data[0..i])?.to_string());
        }
    }

    Err(err_msg("Missing null terminator"))
}
