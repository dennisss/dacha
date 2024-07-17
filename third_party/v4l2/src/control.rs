use core::fmt::Debug;
use std::ptr::read_unaligned;

use base_error::*;

use crate::{bindings::*, utils::read_null_terminated_string};

// TODO: Also port the flags defined in https://www.kernel.org/doc/html/v4.9/media/uapi/v4l/vidioc-queryctrl.html#vidioc-queryctrl. (especially the slider one would be useful for UI rendering).

pub struct ControlDefinition {
    pub(crate) raw: v4l2_queryctrl,
    pub(crate) menu_items: Vec<v4l2_querymenu>,
}

impl ControlDefinition {
    pub fn name(&self) -> Result<String> {
        read_null_terminated_string(&self.raw.name)
    }

    /// Generates a debug string.
    pub fn to_string(&self) -> Result<String> {
        if self.raw.type_ == v4l2_ctrl_type::V4L2_CTRL_TYPE_CTRL_CLASS.0 {
            return Ok(format!("[{}]", self.name()?));
        }

        let mut out = format!(
            "{} (min: {}, max: {})",
            self.name()?,
            self.raw.minimum,
            self.raw.maximum
        );

        for item in &self.menu_items {
            let index: u32 = item.index;

            let value = {
                // Safety of these unsafe statements is assured by the check on the
                // corresponding control type.
                if self.raw.type_ == v4l2_ctrl_type::V4L2_CTRL_TYPE_MENU.0 {
                    read_null_terminated_string(unsafe { &item.__bindgen_anon_1.name })?
                } else {
                    format!("{}", unsafe { item.__bindgen_anon_1.value })
                }
            };

            out.push_str(&format!("\n  - #{}: {}", index, value));
        }

        Ok(out)
    }
}
