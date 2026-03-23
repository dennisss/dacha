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


/// See https://www.kernel.org/doc/html/v4.9/media/uapi/v4l/control.html
pub(crate) fn list_controls(file: &LocalFile) -> Result<Vec<ControlDefinition>> {
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

pub(crate) fn get_control_value(file: &LocalFile, control_definition: &ControlDefinition) -> Result<i32> {
    let mut ctrl = v4l2_control {
        id: control_definition.raw.id,
        value: 0,
    };

    unsafe { vidioc_g_ctrl(file.as_raw_fd(), &mut ctrl) }?;

    Ok(ctrl.value)
}

pub(crate) fn set_control_value(
    file: &LocalFile,
    control_definition: &ControlDefinition,
    value: i32,
) -> Result<()> {
    let mut ctrl = v4l2_control {
        id: control_definition.raw.id,
        value,
    };

    unsafe { vidioc_s_ctrl(file.as_raw_fd(), &mut ctrl) }?;

    Ok(())
}