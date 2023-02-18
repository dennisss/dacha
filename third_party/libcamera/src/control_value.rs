use std::fmt::Debug;
use std::pin::Pin;

use paste::paste;

use crate::bindings::{Rectangle, Size};
use crate::ffi;
use crate::ffi::ControlType;

pub use ffi::ControlValue;

impl ControlValue {
    pub fn cast<'a, T: FromControlValue<'a> + ?Sized>(&'a self) -> Option<T::Target> {
        T::from_value(self)
    }
}

impl Debug for ControlValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", ffi::control_value_to_string(self))
    }
}

pub trait FromControlValue<'a> {
    type Target;

    /// Attempts to convert the opaque ControlValue into this type.
    ///
    /// If there is a type mismatch, None will be returned.
    ///
    /// This will internally be implemented with the C++ ControlValue::get(),
    /// but we check that the types match on the Rust size rather than having
    /// ControlValue::get() throw an exception on type mismatch.
    fn from_value(value: &'a ControlValue) -> Option<Self::Target>;
}

impl<'a> FromControlValue<'a> for ControlValue {
    type Target = &'a ControlValue;

    fn from_value(value: &'a ControlValue) -> Option<Self::Target> {
        Some(value)
    }
}

pub trait AssignToControlValue {
    fn assign_to_value(&self, value: Pin<&mut ffi::ControlValue>);
}

// TODO: Consider not using standard types like AsRef and From as that may hide
// the fact that these can fail.
macro_rules! impl_control_value_type {
    ($typ:ident, $ffi_typ:ident, $control_typ:ident) => {
        paste! {
            impl<'a> FromControlValue<'a> for $typ {
                type Target = Self;

                fn from_value(value: &'a ControlValue) -> Option<Self> {
                    if value.typ() != ControlType::$control_typ || value.is_array() {
                        return None;
                    }

                    Some(value.[<get_ $ffi_typ>]())
                }
            }

            impl<'a> FromControlValue<'a> for [$typ] {
                type Target = &'a Self;

                fn from_value(value: &'a ControlValue) -> Option<&'a Self> {
                    if value.typ() != ControlType::$control_typ || !value.is_array() {
                        return None;
                    }

                    Some(ffi::[<control_value_get_ $ffi_typ _array>](value))
                }
            }

            impl<'a, const LEN: usize> FromControlValue<'a> for [$typ; LEN] {
                type Target = &'a Self;

                fn from_value(value: &'a ControlValue) -> Option<&'a Self> {
                    value.cast::<[$typ]>().and_then(|v| v.try_into().ok())
                }
            }

            impl AssignToControlValue for $typ {
                fn assign_to_value(&self, value: Pin<&mut ControlValue>) {
                    value.[<set_ $ffi_typ>](self);
                }
            }

            impl AssignToControlValue for [$typ] {
                fn assign_to_value(&self, value: Pin<&mut ControlValue>) {
                   ffi::[<control_value_set_ $ffi_typ _array>](value, self);
                }
            }

            impl<const LEN: usize> AssignToControlValue for [$typ; LEN] {
                fn assign_to_value(&self, value: Pin<&mut ControlValue>) {
                   ffi::[<control_value_set_ $ffi_typ _array>](value, &self[..]);
                }
            }
        }
    };
}

impl_control_value_type!(bool, bool, ControlTypeBool);
impl_control_value_type!(u8, byte, ControlTypeByte);
impl_control_value_type!(i32, i32, ControlTypeInteger32);
impl_control_value_type!(i64, i64, ControlTypeInteger64);
impl_control_value_type!(f32, float, ControlTypeFloat);
impl_control_value_type!(Rectangle, rectangle, ControlTypeRectangle);
impl_control_value_type!(Size, size, ControlTypeSize);

impl<'a> FromControlValue<'a> for String {
    type Target = Self;

    fn from_value(value: &'a ControlValue) -> Option<Self::Target> {
        if value.typ() != ControlType::ControlTypeString || value.is_array() {
            return None;
        }

        Some(value.get_string())
    }
}

impl<'a> FromControlValue<'a> for Vec<String> {
    type Target = Self;

    fn from_value(value: &'a ControlValue) -> Option<Self::Target> {
        if value.typ() != ControlType::ControlTypeString || !value.is_array() {
            return None;
        }

        Some(ffi::control_value_get_string_array(value))
    }
}

impl AssignToControlValue for String {
    fn assign_to_value(&self, value: Pin<&mut ControlValue>) {
        ffi::control_value_set_string(value, self);
    }
}

// TODO: AssignToControlValue for &[String]

#[derive(Debug, Clone)]
pub enum ControlValueEnum {
    None,
    Primitive(ControlPrimitiveValue),
    Array(ControlArrayValue),
}

#[derive(Debug, Clone)]
pub enum ControlPrimitiveValue {
    Bool(bool),
    Byte(u8),
    Int32(i32),
    Int64(i64),
    Float(f32),
    Rectangle(Rectangle),
    Size(Size),
    String(String),
}

#[derive(Debug, Clone)]
pub enum ControlArrayValue {
    Bool(Vec<bool>),
    Byte(Vec<u8>),
    Int32(Vec<i32>),
    Int64(Vec<i64>),
    Float(Vec<f32>),
    Rectangle(Vec<Rectangle>),
    Size(Vec<Size>),
    String(Vec<String>),
}

impl ControlValueEnum {
    pub fn is_none(&self) -> bool {
        if let ControlValueEnum::None = self {
            true
        } else {
            false
        }
    }
}

impl<'a> FromControlValue<'a> for ControlValueEnum {
    type Target = Self;

    fn from_value(value: &ControlValue) -> Option<Self> {
        if value.is_array() {
            Some(ControlValueEnum::Array(match value.typ() {
                ControlType::ControlTypeNone => return Some(ControlValueEnum::None),
                ControlType::ControlTypeBool => {
                    ControlArrayValue::Bool(value.cast::<[bool]>().unwrap().to_vec())
                }
                ControlType::ControlTypeByte => {
                    ControlArrayValue::Byte(value.cast::<[u8]>().unwrap().to_vec())
                }
                ControlType::ControlTypeInteger32 => {
                    ControlArrayValue::Int32(value.cast::<[i32]>().unwrap().to_vec())
                }
                ControlType::ControlTypeInteger64 => {
                    ControlArrayValue::Int64(value.cast::<[i64]>().unwrap().to_vec())
                }
                ControlType::ControlTypeFloat => {
                    ControlArrayValue::Float(value.cast::<[f32]>().unwrap().to_vec())
                }
                ControlType::ControlTypeString => {
                    ControlArrayValue::String(value.cast::<Vec<String>>().unwrap())
                }
                ControlType::ControlTypeRectangle => {
                    ControlArrayValue::Rectangle(value.cast::<[Rectangle]>().unwrap().to_vec())
                }
                ControlType::ControlTypeSize => {
                    ControlArrayValue::Size(value.cast::<[Size]>().unwrap().to_vec())
                }
                _ => return None,
            }))
        } else {
            Some(ControlValueEnum::Primitive(match value.typ() {
                ControlType::ControlTypeNone => return Some(ControlValueEnum::None),
                ControlType::ControlTypeBool => {
                    ControlPrimitiveValue::Bool(value.cast::<bool>().unwrap())
                }
                ControlType::ControlTypeByte => {
                    ControlPrimitiveValue::Byte(value.cast::<u8>().unwrap())
                }
                ControlType::ControlTypeInteger32 => {
                    ControlPrimitiveValue::Int32(value.cast::<i32>().unwrap())
                }
                ControlType::ControlTypeInteger64 => {
                    ControlPrimitiveValue::Int64(value.cast::<i64>().unwrap())
                }
                ControlType::ControlTypeFloat => {
                    ControlPrimitiveValue::Float(value.cast::<f32>().unwrap())
                }
                ControlType::ControlTypeString => {
                    ControlPrimitiveValue::String(value.cast::<String>().unwrap())
                }
                ControlType::ControlTypeRectangle => {
                    ControlPrimitiveValue::Rectangle(value.cast::<Rectangle>().unwrap())
                }
                ControlType::ControlTypeSize => {
                    ControlPrimitiveValue::Size(value.cast::<Size>().unwrap())
                }
                _ => {
                    return None;
                }
            }))
        }
    }
}

impl AssignToControlValue for ControlValueEnum {
    fn assign_to_value(&self, value: Pin<&mut ControlValue>) {
        match self {
            ControlValueEnum::None => todo!(),
            ControlValueEnum::Primitive(p) => match p {
                ControlPrimitiveValue::Bool(v) => v.assign_to_value(value),
                ControlPrimitiveValue::Byte(v) => v.assign_to_value(value),
                ControlPrimitiveValue::Int32(v) => v.assign_to_value(value),
                ControlPrimitiveValue::Int64(v) => v.assign_to_value(value),
                ControlPrimitiveValue::Float(v) => v.assign_to_value(value),
                ControlPrimitiveValue::Rectangle(v) => v.assign_to_value(value),
                ControlPrimitiveValue::Size(v) => v.assign_to_value(value),
                ControlPrimitiveValue::String(_) => todo!(),
                //
            },
            ControlValueEnum::Array(_) => todo!(),
        }
    }
}
