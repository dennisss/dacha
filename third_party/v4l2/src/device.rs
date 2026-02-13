use std::{collections::HashSet, sync::Arc};

use base_error::*;
use base_util::null_terminated::read_null_terminated_string;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::AsyncVariable;
use executor::ExecutorPollingContext;
use file::LocalPathBuf;
use file::{LocalFile, LocalFileOpenOptions, LocalPath, DeviceNumber};
use sys::EpollEvents;
use sys::Errno;

use crate::io::*;
use crate::stream::*;
use crate::ControlDefinition;
use crate::{bindings::*, ControlMenuItem};

pub struct Device {
    handle: Arc<DeviceHandle>,

    /// Stream types which have already been created using
    /// new_capture_stream/new_output_stream/etc. This is used to ensure that
    /// the same stream isn't created twice.
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

    pub path: LocalPathBuf,

    pub capability: v4l2_capability,

    device_num: DeviceNumber
}

impl Device {
    /// Enumerates all devices registered in the system.
    ///
    /// Note that V4L2 devices have global properties so one device can't be
    /// shared across multiple processes. But, V4L2 won't stop us from opening
    /// and reading from a device even if another application already has it
    /// open. The main exception to this is M2M devices which do have
    /// per-instance properties.
    pub async fn list() -> Result<Vec<Self>> {
        let mut out = vec![];
        for entry in file::read_dir("/dev")? {
            if !entry.name().starts_with("video") {
                continue;
            }

            let path = LocalPath::new("/dev").join(entry.name());

            out.push(Self::open(path).await?);
        }

        // TODO: Sort by the number.

        out.sort_by(|a, b| a.path().as_str().cmp(b.path().as_str()));

        Ok(out)
    }

    pub async fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let file = file::LocalFile::open_with_options(
            path,
            &LocalFileOpenOptions::new()
                .read(true)
                .write(true)
                .non_blocking(true),
        )?;

        let mut capability = v4l2_capability::default();
        unsafe { vidioc_querycap(file.as_raw_fd(), &mut capability) }?;

        let meta = file.metadata().await?;
        let device_num = meta.represented_device();

        let shared = Arc::new(DeviceShared {
            file: AsyncVariable::new(file),
            capability,
            path: path.to_owned(),
            device_num,
        });

        Ok(Self {
            handle: Arc::new(DeviceHandle {
                // TODO: We don't need to start polling until we have at least one stream open.
                polling_task: ChildTask::spawn(Self::polling_thread(shared.clone())),
                shared,
            }),
            streams: HashSet::new(),
        })
    }

    pub fn path(&self) -> &LocalPath {
        &self.handle.shared.path
    }

    pub fn device_num(&self) -> DeviceNumber {
        self.handle.shared.device_num
    }

    pub async fn print_capabiliites(&self) -> Result<()> {
        let file = self.handle.shared.file.lock().await?.read_exclusive();

        /*
        Important things in caps.device_caps:
        V4L2_CAP_STREAMING - needed to support mmap

        V4L2_CAP_VIDEO_CAPTURE
        V4L2_CAP_VIDEO_CAPTURE_MPLANE

        V4L2_CAP_VIDEO_OUTPUT
        V4L2_CAP_VIDEO_OUTPUT_MPLANE

        V4L2_CAP_VIDEO_M2M ?

        */

        let caps = &self.handle.shared.capability;

        println!("Driver: {}", read_null_terminated_string(&caps.driver)?);
        println!("Card: {}", read_null_terminated_string(&caps.card)?);
        println!("Bus Info: {}", read_null_terminated_string(&caps.bus_info)?);

        println!(
            "Driver Version: {}.{}.{}",
            (caps.version >> 16) & 0xFF,
            (caps.version >> 8) & 0xFF,
            caps.version & 0xFF
        );

        println!("Capabilities: {}", caps.capabilities);
        println!("Device Capabilities: {}", caps.device_caps);

        if self.supports_streaming() {
            println!("Streaming!");
        }

        if self.is_m2m() {
            println!("Memory to Memory!");
        }

        Ok(())
    }

    pub fn supports_streaming(&self) -> bool {
        let caps = self.handle.shared.capability.capabilities;
        caps & V4L2_CAP_STREAMING != 0
    }

    /// Checks if this is an M2M device. M2M devices can be opened multiple
    /// times by different application.
    ///
    /// See https://www.kernel.org/doc/html/v5.6/media/uapi/v4l/dev-mem2mem.html
    pub fn is_m2m(&self) -> bool {
        let caps = self.handle.shared.capability.capabilities;
        caps & (V4L2_CAP_VIDEO_M2M | V4L2_CAP_VIDEO_M2M_MPLANE) != 0
    }

    pub fn supports_capture_stream(&self) -> bool {
        let caps = self.handle.shared.capability.capabilities;
        (caps & (V4L2_CAP_VIDEO_CAPTURE | V4L2_CAP_VIDEO_CAPTURE_MPLANE) != 0) || self.is_m2m()
    }

    pub fn new_capture_stream(&self) -> Result<UnconfiguredStream> {
        let caps = self.handle.shared.capability.capabilities;

        let typ = {
            if caps & (V4L2_CAP_VIDEO_CAPTURE_MPLANE | V4L2_CAP_VIDEO_M2M_MPLANE) != 0 {
                v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE
            } else {
                v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_CAPTURE
            }
        };

        self.new_stream(typ)
    }

    pub fn supports_output_stream(&self) -> bool {
        let caps = self.handle.shared.capability.capabilities;
        (caps & (V4L2_CAP_VIDEO_OUTPUT | V4L2_CAP_VIDEO_OUTPUT_MPLANE) != 0) || self.is_m2m()
    }

    pub fn new_output_stream(&self) -> Result<UnconfiguredStream> {
        let caps = self.handle.shared.capability.capabilities;

        let typ = {
            if caps & (V4L2_CAP_VIDEO_OUTPUT_MPLANE | V4L2_CAP_VIDEO_M2M_MPLANE) != 0 {
                v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE
            } else {
                v4l2_buf_type::V4L2_BUF_TYPE_VIDEO_OUTPUT
            }
        };

        self.new_stream(typ)
    }

    // NOTE: We don't expose this directly to users to ensure that the other methods
    // that normalize usage of _MPLANE types when available are used.
    fn new_stream(&self, typ: v4l2_buf_type) -> Result<UnconfiguredStream> {
        // TOOD: Add back this check (ideally while still only requiring &self access).

        // if !self.streams.insert(typ) {
        //     return Err(format_err!(
        //         "Already configuring a stream with buffer type {:?}",
        //         typ
        //     ));
        // }

        Ok(UnconfiguredStream {
            device: self.handle.clone(),
            typ,
        })
    }

    pub async fn list_inputs(&self) -> Result<Vec<v4l2_input>> {
        let file = self.handle.shared.file.lock().await?.read_exclusive();

        let mut out = vec![];

        loop {
            let mut raw = v4l2_input::default();
            raw.index = out.len() as u32;

            match unsafe { vidioc_enuminput(file.as_raw_fd(), &mut raw) } {
                Ok(i) => {
                    assert_eq!(i, 0);
                }
                Err(Errno::EINVAL) => break,
                Err(e) => return Err(e.into()),
            }

            println!("GOT INPUT");

            out.push(raw);
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
                Err(e) => break,
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

    // TODO: Also implement ext_ctrl enumeration.

    /// See https://www.kernel.org/doc/html/v4.9/media/uapi/v4l/control.html
    pub async fn list_controls(&self) -> Result<Vec<ControlDefinition>> {
        let file = self.handle.shared.file.lock().await?.read_exclusive();

        let mut out = vec![];

        let mut raw = v4l2_queryctrl::default();
        raw.id = 0 | V4L2_CTRL_FLAG_NEXT_CTRL;

        loop {
            match unsafe { vidioc_queryctrl(file.as_raw_fd(), &mut raw) } {
                Ok(i) => {
                    assert_eq!(i, 0);
                }
                Err(Errno::EINVAL) => break,
                Err(e) => break,
            };

            let mut menu_items = vec![];
            if raw.type_ == v4l2_ctrl_type::V4L2_CTRL_TYPE_MENU.0
                || raw.type_ == v4l2_ctrl_type::V4L2_CTRL_TYPE_INTEGER_MENU.0
            {
                let mut menu_item = v4l2_querymenu::default();
                menu_item.id = raw.id;

                for index in raw.minimum..(raw.maximum + 1) {
                    menu_item.index = index as u32;
                    match unsafe { vidioc_querymenu(file.as_raw_fd(), &mut menu_item) } {
                        Ok(v) => {}
                        Err(e) => continue,
                    }

                    menu_items.push(ControlMenuItem {
                        control_type: raw.type_,
                        raw: menu_item.clone(),
                    });
                }
            }

            out.push(ControlDefinition { raw, menu_items });
            raw.id |= V4L2_CTRL_FLAG_NEXT_CTRL
        }

        Ok(out)
    }

    pub async fn get_control_value(&self, control_definition: &ControlDefinition) -> Result<i32> {
        let file = self.handle.shared.file.lock().await?.read_exclusive();

        let mut ctrl = v4l2_control {
            id: control_definition.raw.id,
            value: 0,
        };

        unsafe { vidioc_g_ctrl(file.as_raw_fd(), &mut ctrl) }?;

        Ok(ctrl.value)
    }

    pub async fn set_control_value(
        &self,
        control_definition: &ControlDefinition,
        value: i32,
    ) -> Result<()> {
        let file = self.handle.shared.file.lock().await?.read_exclusive();

        let mut ctrl = v4l2_control {
            id: control_definition.raw.id,
            value,
        };

        unsafe { vidioc_s_ctrl(file.as_raw_fd(), &mut ctrl) }?;

        Ok(())
    }

    // vidioc_enumaudio
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
                // (Also ensure all Rust side waiters are aware when this
                // happens). eprintln!("Unknown poll events
                // received: {:?}", events);
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
