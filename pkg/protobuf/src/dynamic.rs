use std::collections::HashMap;

use protobuf_core::{EnumValue, FieldNumber};

use crate::reflection::{Reflect, Reflection, ReflectionMut};
use crate::BytesField;

/*
Basically need a descriptor pool.

How can the compiler tell all the descriptors needed to generate a message?
-

Basically how to flatten a message to all

*/

pub struct DynamicMessage {
    fields: HashMap<FieldNumber, DynamicField>,
    /*
    Need a descriptor for:
    - Knowing field names (for reflection)
    - Parsing from binary (need to know how to parse each type of mesage)
    */
}

enum DynamicField {
    Singular(DynamicValue),
    Repeated(DynamicRepeatedField),
}

enum DynamicValue {
    Primitive(DynamicPrimitiveValue),
    Enum(DynamicEnum),
    Message(DynamicMessage),
}

struct DynamicRepeatedField {
    values: Vec<DynamicValue>,
}

struct DynamicEnum {
    value: EnumValue,
}

enum DynamicPrimitiveValue {
    Double(f64),
    Float(f32),
    Int32(i32),
    Int64(i64),
    Uint32(u32),
    Uint64(u64),
    Sint32(i32),
    Sint64(i64),
    Fixed32(u32),
    Fixed64(u64),
    Sfixed32(i32),
    Sfixed64(i64),
    Bool(bool),
    String(String),
    Bytes(BytesField),
}

impl Reflect for DynamicPrimitiveValue {
    fn reflect(&self) -> Option<Reflection> {
        Some(match self {
            DynamicPrimitiveValue::Double(v) => Reflection::F64(v),
            DynamicPrimitiveValue::Float(v) => Reflection::F32(v),
            DynamicPrimitiveValue::Int32(v) => Reflection::I32(v),
            DynamicPrimitiveValue::Int64(v) => Reflection::I64(v),
            DynamicPrimitiveValue::Uint32(v) => Reflection::U32(v),
            DynamicPrimitiveValue::Uint64(v) => Reflection::U64(v),
            DynamicPrimitiveValue::Sint32(v) => Reflection::I32(v),
            DynamicPrimitiveValue::Sint64(v) => Reflection::I64(v),
            DynamicPrimitiveValue::Fixed32(v) => Reflection::U32(v),
            DynamicPrimitiveValue::Fixed64(v) => Reflection::U64(v),
            DynamicPrimitiveValue::Sfixed32(v) => Reflection::I32(v),
            DynamicPrimitiveValue::Sfixed64(v) => Reflection::I64(v),
            DynamicPrimitiveValue::Bool(v) => Reflection::Bool(v),
            DynamicPrimitiveValue::String(v) => Reflection::String(v),
            DynamicPrimitiveValue::Bytes(v) => Reflection::Bytes(v.as_ref()),
        })
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        todo!()
    }
}
