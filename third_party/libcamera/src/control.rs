use std::marker::PhantomData;
use std::ops::Deref;
use std::pin::Pin;

use crate::ffi;
use crate::ffi::ControlId;
use crate::{AssignToControlValue, FromControlValue, Rectangle, Size};

#[derive(Clone, Copy)]
pub struct Control<T: ?Sized> {
    id: fn() -> &'static ControlId,
    t: PhantomData<T>,
}

// The last two trait bounds here are mainly to catch any unimplemented traits
// for the generated code.
impl<T: ?Sized + AssignToControlValue + for<'a> FromControlValue<'a>> Control<T> {
    /// This is unsafe because we assume that T is compatible with id.typ().
    ///
    /// NOTE: This should only be used in auto generated code in the
    /// crate::controls module.
    pub(crate) const unsafe fn new(id: fn() -> &'static ControlId) -> Self {
        Self { id, t: PhantomData }
    }
}

impl<T: ?Sized> Deref for Control<T> {
    type Target = ControlId;

    fn deref(&self) -> &Self::Target {
        (self.id)()
    }
}

/// WARNING: This macro is unsafe and must be scoped only to this crate.
macro_rules! control {
    ($(#[$meta:meta])* $name:ident, $t:ty, $ns:ident) => {
        control!($(#[$meta])* $name, $t, $crate::bindings::$ns::$name);
    };
    ($(#[$meta:meta])* $name:ident, $t:ty, $ns:ident :: $sns:ident) => {
        control!($(#[$meta])* $name, $t, $crate::bindings::$ns::$sns::$name);
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

pub mod controls {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    use super::*;

    include!(concat!(env!("OUT_DIR"), "/controls.rs"));
}

pub mod properties {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    use super::*;

    include!(concat!(env!("OUT_DIR"), "/properties.rs"));
}
