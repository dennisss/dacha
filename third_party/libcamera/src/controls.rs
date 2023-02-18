#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::pin::Pin;

use crate::ffi;
use crate::{AssignToControlValue, Control, FromControlValue, Rectangle, Size};

/// WARNING: This macro is unsafe and must be scoped only to this crate.
macro_rules! control {
    ($(#[$meta:meta])* $name:ident, $t:ty, stable) => {
        control!($(#[$meta])* $name, $t, $crate::bindings::controls::$name);
    };
    ($(#[$meta:meta])* $name:ident, $t:ty, draft) => {
        control!($(#[$meta])* $name, $t, $crate::bindings::controls::draft::$name);
    };
    ($(#[$meta:meta])* $name:ident, $t:ty, $extern_var:expr) => {
        $(#[$meta])*
        pub const $name: Control<$t> =
            unsafe { Control::new(|| ::core::mem::transmute(&$extern_var)) };
    };
}

// Keep scoped to this crate only.
macro_rules! control_enum {
    ($name:ident $t:ty { $($(#[$meta:meta])* $case:ident = $val:expr,)* }) => {
        #[derive(Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(transparent)]
        pub struct $name {
            value: $t,
        }

        impl $name {
            $(
                $(#[$meta])*
                pub const $case: Self = Self::new($val);
            )*

            const fn new(value: $t) -> Self {
                Self { value }
            }
        }

        impl<'a> FromControlValue<'a> for $name {
            type Target = Self;

            fn from_value(value: &'a ffi::ControlValue) -> Option<Self> {
                <$t as FromControlValue>::from_value(value).map(|v| Self::new(v))
            }
        }

        impl AssignToControlValue for $name {
            fn assign_to_value(&self, value: Pin<&mut ffi::ControlValue>) {
                <$t>::assign_to_value(&self.value, value)
            }
        }

        // TODO: also need stringification
    };
}

include!(concat!(env!("OUT_DIR"), "/controls.rs"));
