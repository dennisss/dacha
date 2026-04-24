use std::{collections::HashSet, sync::Arc};

use base_error::*;
use base_util::null_terminated::read_null_terminated_string;
use executor::child_task::ChildTask;
use executor::lock;
use executor::sync::AsyncMutex;
use executor::ExecutorPollingContext;
use file::LocalPathBuf;
use file::{LocalFile, LocalFileOpenOptions, LocalPath, DeviceNumber};
use sys::EpollEvents;
use sys::Errno;

use crate::io::*;
use crate::stream::*;
use crate::ControlDefinition;
use crate::{bindings::*, ControlMenuItem};
use crate::device::Controllable;


pub struct SubDevice {
    file: AsyncMutex<LocalFile>,
    capabilities: v4l2_subdev_capability,
    path: LocalPathBuf,
    device_num: DeviceNumber,
}

impl SubDevice {
    pub async fn list() -> Result<Vec<Self>> {
        let mut out = vec![];
        for entry in file::read_dir("/dev")? {
            if !entry.name().starts_with("v4l-subdev") {
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

        let mut capabilities = v4l2_subdev_capability::default();
        unsafe { vidioc_subdev_querycap(file.as_raw_fd(), &mut capabilities) }?;

        let meta = file.metadata().await?;
        let device_num = meta.represented_device();

        Ok(Self {
            file: AsyncMutex::new(file),
            capabilities,
            path: path.to_owned(),
            device_num
        })
    }

    pub fn path(&self) -> &LocalPath {
        &self.path
    }

    pub fn device_num(&self) -> DeviceNumber {
        self.device_num
    }

    pub async fn format(&self, pad: usize) -> Result<v4l2_subdev_format> {
        let file = self.file.lock().await?.read_exclusive();

        let mut fmt = v4l2_subdev_format::default();
        fmt.pad = pad as u32;
        fmt.which = v4l2_subdev_format_whence::V4L2_SUBDEV_FORMAT_ACTIVE.0 as u32;
        unsafe { vidioc_subdev_g_fmt(file.as_raw_fd(), &mut fmt) }?;
        Ok(fmt)
    }

    pub async fn set_format(&self, pad: usize, format: &v4l2_subdev_format) -> Result<()> {
        let file = self.file.lock().await?.read_exclusive();
        
        let mut fmt = format.clone();
        fmt.pad = pad as u32;
        fmt.which = v4l2_subdev_format_whence::V4L2_SUBDEV_FORMAT_ACTIVE.0 as u32;
        unsafe { vidioc_subdev_s_fmt(file.as_raw_fd(), &mut fmt) }?;
        Ok(())
    }
}

#[async_trait]
impl Controllable for SubDevice {
    async fn list_controls(&self) -> Result<Vec<ControlDefinition>> {
        let file = self.file.lock().await?.read_exclusive();
        crate::control_helpers::list_controls(&file)
    }

    async fn get_control_value(&self, control_definition: &ControlDefinition) -> Result<i32> {
        let file = self.file.lock().await?.read_exclusive();
        crate::control_helpers::get_control_value(&file, control_definition)
    }

    async fn set_control_value(
        &self,
        control_definition: &ControlDefinition,
        value: i32,
    ) -> Result<()> {
        let file = self.file.lock().await?.read_exclusive();
        crate::control_helpers::set_control_value(&file, control_definition, value)
    }
}
