use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use common::errors::*;
use common::list::Appendable;

use crate::bytes::*;
use crate::codecs::*;
use crate::message::{Enum, Message, MessagePtr};
use crate::reflection::*;
use crate::types::{EnumValue, FieldNumber};
use crate::wire::*;

/// Value of a message field which has a type known only at runtime.
#[derive(Clone, PartialEq)]
pub enum Value {
    Singular(SingularValue),
    Repeated(RepeatedValues),
}

impl Value {
    pub fn new(default_value: SingularValue, is_repeated: bool) -> Self {
        if is_repeated {
            Self::Repeated(RepeatedValues::new(default_value))
        } else {
            Self::Singular(default_value)
        }
    }

    pub fn parse_merge(
        &mut self,
        // TODO: Use WireValue here.
        wire_field: &WireField,
    ) -> WireResult<()> {
        match self {
            Self::Singular(v) => {
                v.parse_merge(wire_field)?;
            }
            Self::Repeated(v) => {
                v.parse_merge(wire_field)?;
            }
        }

        Ok(())
    }

    pub fn serialize_to<A: Appendable<Item = u8> + ?Sized>(
        &self,
        field_number: FieldNumber,
        out: &mut A,
    ) -> Result<()> {
        // TODO: Ignore fields with default values in proto3 (by using the sparse
        // serializers).
        // ^ Also implement in proto2 with custom values.

        // TODO: Implement optionally using the packed format.

        match self {
            Self::Singular(v) => {
                v.serialize_singular_value(field_number, out)?;
            }
            Self::Repeated(v) => {
                v.serialize(field_number, out)?;
            }
        }

        Ok(())
    }
}

impl Reflect for Value {
    fn reflect(&self) -> Reflection {
        match self {
            Value::Singular(v) => v.reflect(),
            Value::Repeated(v) => Reflection::Repeated(v),
        }
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        match self {
            Value::Singular(v) => v.reflect_mut(),
            Value::Repeated(v) => ReflectionMut::Repeated(v),
        }
    }
}

macro_rules! define_primitive_values {
    ($v:ident, $( $name:ident ( $t:ty ) => $reflection_variant:ident ( $reflection_value:expr, $reflection_mut:expr, $serialize_value:expr ) ),*) => {

        /// Singular value of a message field with unknown type.
        pub enum SingularValue {
            Enum(Box<dyn Enum>),
            // TODO: If the type is one that is already linked to the current binary, parse into a 'static' type instead of a dynamic one.
            Message(Box<dyn MessageReflection>), // TODO: Use a MessagePtr here.
            $( $name($t) ),*
        }

        impl Clone for SingularValue {
            fn clone(&self) -> Self {
                match self {
                    SingularValue::Enum(v) => SingularValue::Enum(v.box_clone()),
                    SingularValue::Message(v) => SingularValue::Message(MessageReflection::box_clone2(v.as_ref())),
                    $( SingularValue::$name(v) => SingularValue::$name(v.clone()) ),*
                }
            }
        }

        impl PartialEq for SingularValue {
            fn eq(&self, other: &Self) -> bool {
                match self {
                    SingularValue::Enum(v) => {
                        // TODO: also match to integer types? (or at least enfore that types exactly match)
                        match other {
                            SingularValue::Enum(v2) => v.value() == v2.value(),
                            _ => { false }
                        }
                    },
                    SingularValue::Message(v) => {
                        match other {
                            SingularValue::Message(v2) => v.message_equals(v2.as_ref()),
                            _ => { false }
                        }
                    },
                    $( SingularValue::$name(v) => match other { SingularValue::$name(v2) => { v == v2 } _ => { false } } ),*
                }
            }
        }

        impl Reflect for SingularValue {
            fn reflect(&self) -> Reflection {
                match self {
                    SingularValue::Enum(v) => Reflection::Enum(v.as_ref()),
                    SingularValue::Message(v) => Reflection::Message(v.as_ref()),
                    $( SingularValue::$name($v) => Reflection::$reflection_variant($reflection_value) ),*
                }
            }

            fn reflect_mut(&mut self) -> ReflectionMut {
                match self {
                    SingularValue::Enum(v) => ReflectionMut::Enum(v.as_mut()),
                    SingularValue::Message(v) => ReflectionMut::Message(v.as_mut()),
                    $( SingularValue::$name($v) => ReflectionMut::$reflection_variant($reflection_mut) ),*
                }
            }
        }

        impl SingularValue {
            fn parse_merge(&mut self, wire_field: &WireField) -> WireResult<()> {
                Ok(match self {
                    Self::Message(m) => {
                        MessageCodec::parse_into(wire_field, m.as_mut())?;
                    }
                    Self::Enum(e) => {
                        EnumCodec::parse_into(wire_field, e.as_mut())?;
                    }
                    $(
                    Self::$name(v) => {
                        *v = <concat_idents!($name, Codec)>::parse(wire_field)?;
                    }
                    )*
                })
            }

            fn serialize_singular_value<A: Appendable<Item = u8> + ?Sized>(&self, field_num: FieldNumber, out: &mut A) -> Result<()> {
                match self {
                    $(
                    SingularValue::$name($v) => {
                        <concat_idents!($name, Codec)>::serialize_sparse(field_num, $serialize_value, out)?
                    }
                    ),*
                    SingularValue::Enum(v) => {
                        EnumCodec::serialize_sparse(field_num, v.as_ref(), out)?
                    }
                    SingularValue::Message(v) => {
                        MessageCodec::serialize(field_num, v.as_ref(), out)?
                    }
                };
                Ok(())
            }
        }

        /// Repeated value of a message field with unknown type.
        pub enum RepeatedValues {
            Enum {
                values: Vec<Box<dyn Enum>>,

                /// Default value used when appending a new entry to this repeated field.
                default_value: Box<dyn Enum>
            },
            Message {
                values: Vec<MessagePtr<dyn MessageReflection>>,

                /// Default value used when appending a new entry to this repeated field.
                default_value: Box<dyn MessageReflection>
            },
            $( $name { values: Vec<$t>, default_value: $t, } ),*
        }

        impl PartialEq for RepeatedValues {
            fn eq(&self, other: &Self) -> bool {
                match self {
                    Self::Enum { values, .. } => {
                        // TODO: also match to integer types? (or at least enfore that types exactly match)
                        match other {
                            Self::Enum { values: values2, .. } => {
                                if values.len() != values2.len() {
                                    false
                                } else {
                                    for i in 0..values.len() {
                                        if values[i].value() != values2[i].value() {
                                            return false;
                                        }
                                    }

                                    true
                                }
                            },
                            _ => { false }
                        }
                    },
                    Self::Message { values, .. } => {
                        match other {
                            Self::Message { values: values2, .. } => {
                                for i in 0..values.len() {
                                    if !values[i].message_equals(values2[i].as_ref()) {
                                        return false;
                                    }
                                }

                                true
                            },
                            _ => { false }
                        }
                    },
                    $(
                    Self::$name { values, .. } => {
                        match other {
                            Self::$name { values: values2, .. } => { values == values2 }
                            _ => { false } }
                    }
                    ),*
                }
            }
        }

        impl Clone for RepeatedValues {
            fn clone(&self) -> Self {
                match self {
                    RepeatedValues::Message { values, default_value } => Self::Message {
                        values: values.iter().map(|v| MessagePtr::new_boxed(v.box_clone2())).collect(),
                        default_value: MessageReflection::box_clone2(default_value.as_ref())
                    },
                    RepeatedValues::Enum { values, default_value } => Self::Enum {
                        values: values.iter().map(|v| v.box_clone()).collect(),
                        default_value: default_value.box_clone()
                    },
                    $( RepeatedValues::$name { values, default_value } => Self::$name {
                        values: values.clone(),
                        default_value: default_value.clone()
                    } ),*
                }
            }
        }

        impl RepeatedValues {
            fn new(default_value: SingularValue) -> Self {
                match default_value {
                    SingularValue::Message(v) => Self::Message { values: vec![], default_value: v },
                    SingularValue::Enum(v) => Self::Enum { values: vec![], default_value: v },
                    $( SingularValue::$name(v) => Self::$name { values: vec![], default_value: v } ),*
                }
            }

            fn parse_merge(&mut self, wire_field: &WireField) -> WireResult<()> {
                match self {
                    RepeatedValues::Message { values, default_value }  => {
                        // NOTE: Doesn't support packed serialization.
                        let mut val = MessageReflection::box_clone2(default_value.as_ref());
                        MessageCodec::parse_into(wire_field, val.as_mut())?;
                        values.push(MessagePtr::new_boxed(val));
                    }
                    RepeatedValues::Enum { values, default_value } => {
                        for v in EnumCodec::parse_repeated::<AnonymousEnum>(wire_field) {
                            let v = v?;

                            let mut val = default_value.box_clone();
                            val.assign(v.value);
                            values.push(val);
                        }
                    }
                    $(
                        RepeatedValues::$name { values, .. } => {
                            for v in <concat_idents!($name, Codec)>::parse_repeated(wire_field) {
                                values.push(v?);
                            }
                        }
                    ),*
                }

                Ok(())
            }

            fn serialize<A: Appendable<Item = u8> + ?Sized>(&self, field_num: FieldNumber, out: &mut A) -> Result<()> {
                match self {
                    RepeatedValues::Enum { values, .. } => {
                        EnumCodec::serialize_repeated_dyn(field_num, &values[..], out)?;
                    }
                    RepeatedValues::Message { values, .. } => {
                        MessageCodec::serialize_repeated(field_num, &values, out)?;
                    }
                    $(
                    RepeatedValues::$name { values, .. } => {
                        // $serialize_value
                        <concat_idents!($name, Codec)>::serialize_repeated(field_num, &values[..], out)?;
                    }
                    ),*
                };
                Ok(())
            }
        }

        impl RepeatedFieldReflection for RepeatedValues {
            fn reflect_len(&self) -> usize {
                match self {
                    RepeatedValues::Enum { values, .. } => values.len(),
                    RepeatedValues::Message { values, .. } => values.len(),
                    $(
                        RepeatedValues::$name { values, .. } => values.len()
                    ),*
                }
            }

            fn reflect_get(&self, index: usize) -> Option<Reflection> {
                match self {
                    RepeatedValues::Enum { values, .. } => {
                        values.get(index).map(|v| Reflection::Enum(v.as_ref()))
                    },
                    RepeatedValues::Message { values, .. } => {
                        values.get(index).map(|v| Reflection::Message(v.as_ref()))
                    },
                    $(
                        RepeatedValues::$name { values, .. } => {
                            values.get(index).map(|$v| Reflection::$reflection_variant($reflection_value))
                        }
                    ),*
                }
            }

            fn reflect_get_mut(&mut self, index: usize) -> Option<ReflectionMut> {
                match self {
                    RepeatedValues::Enum { values, .. } => {
                        values.get_mut(index).map(|v| ReflectionMut::Enum(v.as_mut()))
                    },
                    RepeatedValues::Message { values, .. } => {
                        values.get_mut(index).map(|v| ReflectionMut::Message(v.as_mut()))
                    },
                    $(
                        RepeatedValues::$name { values, .. } => {
                            values.get_mut(index).map(|$v| ReflectionMut::$reflection_variant($reflection_mut))
                        }
                    ),*
                }
            }

            fn reflect_add(&mut self) -> ReflectionMut {
                match self {
                    RepeatedValues::Enum { values, default_value } => {
                        values.push( default_value.box_clone());
                    },
                    RepeatedValues::Message { values, default_value } => {
                        values.push(MessagePtr::new_boxed(default_value.box_clone2()));
                    },
                    $(
                        RepeatedValues::$name { values, default_value } => {
                            values.push(default_value.clone());
                        }
                    ),*
                }

                self.reflect_get_mut(self.reflect_len() - 1).unwrap()
            }
        }

    };
}

define_primitive_values!(
    v,
    Double(f64) => F64(v, v, *v),
    Float(f32) => F32(v, v, *v),
    Int32(i32) => I32(v, v, *v),
    Int64(i64) => I64(v, v, *v),
    UInt32(u32) => U32(v, v, *v),
    UInt64(u64) => U64(v, v, *v),
    SInt32(i32) => I32(v, v, *v),
    SInt64(i64) => I64(v, v, *v),
    Fixed32(u32) => U32(v, v, *v),
    Fixed64(u64) => U64(v, v, *v),
    SFixed32(i32) => I32(v, v, *v),
    SFixed64(i64) => I64(v, v, *v),
    Bool(bool) => Bool(v, v, *v),
    String(String) => String(v, v, v.as_ref()),
    Bytes(BytesField) => Bytes(v.as_ref(), &mut v.0, &*v)
);

#[derive(Clone)]
struct AnonymousEnum {
    value: EnumValue,
}

impl Enum for AnonymousEnum {
    fn parse(v: EnumValue) -> WireResult<Self>
    where
        Self: Sized,
    {
        Ok(Self { value: v })
    }

    fn parse_name(name: &str) -> WireResult<Self>
    where
        Self: Sized,
    {
        // Can't implement without a descriptor.
        todo!()
    }

    fn name(&self) -> &str {
        todo!()
    }

    fn value(&self) -> EnumValue {
        self.value
    }

    fn assign(&mut self, v: EnumValue) -> WireResult<()> {
        self.value = v;
        Ok(())
    }

    fn assign_name(&mut self, name: &str) -> WireResult<()> {
        todo!()
    }

    fn box_clone(&self) -> Box<dyn Enum> {
        Box::new(self.clone())
    }
}
