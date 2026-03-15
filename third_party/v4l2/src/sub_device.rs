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


pub struct SubDevice {
    file: LocalFile,
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
            file,
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

    /// TODO: THis is identical to the 'Device' code.
    pub fn list_controls(&mut self) -> Result<Vec<ControlDefinition>> {
        let file = &mut self.file;

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
    
    pub fn format(&self, pad: usize) -> Result<v4l2_subdev_format> {
        let mut fmt = v4l2_subdev_format::default();
        fmt.pad = pad as u32;
        fmt.which = v4l2_subdev_format_whence::V4L2_SUBDEV_FORMAT_ACTIVE.0 as u32;
        unsafe { vidioc_subdev_g_fmt(self.file.as_raw_fd(), &mut fmt) }?;
        Ok(fmt)
    }

    pub fn set_format(&mut self, pad: usize, format: &v4l2_subdev_format) -> Result<()> {
        let mut fmt = format.clone();
        fmt.pad = pad as u32;
        fmt.which = v4l2_subdev_format_whence::V4L2_SUBDEV_FORMAT_ACTIVE.0 as u32;
        unsafe { vidioc_subdev_s_fmt(self.file.as_raw_fd(), &mut fmt) }?;
        Ok(())
    }
}