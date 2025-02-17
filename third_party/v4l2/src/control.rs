use core::fmt::Debug;
use std::ptr::read_unaligned;

use base_error::*;
use base_util::null_terminated::read_null_terminated_string;

use crate::bindings::*;

// TODO: Also port the flags defined in https://www.kernel.org/doc/html/v4.9/media/uapi/v4l/vidioc-queryctrl.html#vidioc-queryctrl. (especially the slider one would be useful for UI rendering).

pub struct ControlDefinition {
    pub(crate) raw: v4l2_queryctrl,
    pub(crate) menu_items: Vec<ControlMenuItem>,
}

impl ControlDefinition {
    pub fn name(&self) -> Result<String> {
        read_null_terminated_string(&self.raw.name)
    }

    pub fn flags(&self) -> ControlFlags {
        ControlFlags::from_raw(self.raw.flags)
    }

    pub fn id(&self) -> u32 {
        self.raw.id
    }

    pub fn minimum(&self) -> i32 {
        self.raw.minimum
    }

    pub fn maximum(&self) -> i32 {
        self.raw.maximum
    }

    pub fn default_value(&self) -> i32 {
        self.raw.default_value
    }

    pub fn step(&self) -> i32 {
        self.raw.step
    }

    pub fn menu_items(&self) -> &[ControlMenuItem] {
        &self.menu_items
    }

    pub fn typ(&self) -> ControlType {
        ControlType::from_value(self.raw.type_)
    }

    /// Generates a debug string.
    pub fn to_string(&self) -> Result<String> {
        if self.raw.type_ == v4l2_ctrl_type::V4L2_CTRL_TYPE_CTRL_CLASS.0 {
            return Ok(format!("[{}]", self.name()?));
        }

        let mut out = format!(
            "{} ({:?}; min: {}, max: {}, {})",
            self.name()?,
            self.typ(),
            self.raw.minimum,
            self.raw.maximum,
            self.flags().to_string()
        );

        for item in &self.menu_items {
            out.push_str(&format!("\n  - #{}: {}", item.index(), item.name()?));
        }

        Ok(out)
    }
}

enum_def_with_unknown!(ControlType u32 =>
    CLASS = v4l2_ctrl_type::V4L2_CTRL_TYPE_CTRL_CLASS.0,
    INTEGER = v4l2_ctrl_type::V4L2_CTRL_TYPE_INTEGER.0,
    BOOLEAN = v4l2_ctrl_type::V4L2_CTRL_TYPE_BOOLEAN.0,
    MENU = v4l2_ctrl_type::V4L2_CTRL_TYPE_MENU.0,
    INTEGER_MENU = v4l2_ctrl_type::V4L2_CTRL_TYPE_INTEGER_MENU.0
);

define_bit_flags!(
    ControlFlags u32 {
        DISABLED = V4L2_CTRL_FLAG_DISABLED,
        GRABBED = V4L2_CTRL_FLAG_GRABBED,
        READ_ONLY = V4L2_CTRL_FLAG_READ_ONLY,
        UPDATE = V4L2_CTRL_FLAG_UPDATE,
        INACTIVE = V4L2_CTRL_FLAG_INACTIVE,
        SLIDER = V4L2_CTRL_FLAG_SLIDER,
        WRITE_ONLY = V4L2_CTRL_FLAG_WRITE_ONLY,
        VOLATILE = V4L2_CTRL_FLAG_VOLATILE,
        HAS_PAYLOAD = V4L2_CTRL_FLAG_HAS_PAYLOAD,
        EXECUTE_ON_WRITE = V4L2_CTRL_FLAG_EXECUTE_ON_WRITE,
        MODIFY_LAYOUT = V4L2_CTRL_FLAG_MODIFY_LAYOUT
    }
);

pub struct ControlMenuItem {
    pub(crate) control_type: u32,
    pub(crate) raw: v4l2_querymenu,
}

impl ControlMenuItem {
    pub fn index(&self) -> u32 {
        self.raw.index
    }

    pub fn name(&self) -> Result<String> {
        // Safety of these unsafe statements is assured by the check on the
        // corresponding control type.
        Ok(
            if self.control_type == v4l2_ctrl_type::V4L2_CTRL_TYPE_MENU.0 {
                read_null_terminated_string(unsafe { &self.raw.__bindgen_anon_1.name })?
            } else {
                format!("{}", unsafe { self.raw.__bindgen_anon_1.value })
            },
        )
    }
}
