use std::collections::HashMap;

use common::errors::*;
use protobuf_core::reflection::RepeatedFieldReflection;
use protobuf_core::wire::{WireField, WireFieldIter};
use protobuf_core::{EnumValue, FieldNumber};
use protobuf_descriptor::{FieldDescriptorProto_Label, FieldDescriptorProto_Type};

use crate::descriptor_pool::*;
use crate::reflection::{Reflect, Reflection, ReflectionMut};
use crate::BytesField;

#[derive(Clone)]
pub struct DynamicMessage {
    fields: HashMap<FieldNumber, DynamicField>,
    desc: MessageDescriptor,
}

impl DynamicMessage {
    pub fn new(desc: MessageDescriptor) -> Self {
        Self {
            fields: HashMap::new(),
            desc,
        }
    }

    fn default_value_for_field(field_desc: &FieldDescriptor) -> Result<DynamicValue> {
        use FieldDescriptorProto_Type::*;
        Ok(match field_desc.proto().typ() {
            TYPE_DOUBLE => DynamicValue::Primitive(DynamicPrimitiveValue::Double(0.0)),
            TYPE_FLOAT => DynamicValue::Primitive(DynamicPrimitiveValue::Float(0.0)),
            TYPE_INT64 => DynamicValue::Primitive(DynamicPrimitiveValue::Int64(0)),
            TYPE_UINT64 => DynamicValue::Primitive(DynamicPrimitiveValue::Uint64(0)),
            TYPE_INT32 => DynamicValue::Primitive(DynamicPrimitiveValue::Int32(0)),
            TYPE_FIXED64 => DynamicValue::Primitive(DynamicPrimitiveValue::Uint64(0)),
            TYPE_FIXED32 => DynamicValue::Primitive(DynamicPrimitiveValue::Uint32(0)),
            TYPE_BOOL => DynamicValue::Primitive(DynamicPrimitiveValue::Bool(false)),
            TYPE_STRING => DynamicValue::Primitive(DynamicPrimitiveValue::String(String::new())),
            TYPE_GROUP => {
                todo!()
            }
            TYPE_MESSAGE | TYPE_ENUM => match field_desc.find_type() {
                Some(TypeDescriptor::Message(m)) => {
                    let val = DynamicMessage::new(m);
                    DynamicValue::Message(val)
                }
                Some(TypeDescriptor::Enum(e)) => {
                    let val = DynamicEnum::new(e);
                    DynamicValue::Enum(val)
                }
                _ => return Err(err_msg("Unknown type in descriptor")),
            },
            TYPE_BYTES => DynamicValue::Primitive(DynamicPrimitiveValue::Bytes(Vec::new().into())),
            TYPE_UINT32 => DynamicValue::Primitive(DynamicPrimitiveValue::Uint32(0)),
            TYPE_SFIXED32 => DynamicValue::Primitive(DynamicPrimitiveValue::Int32(0)),
            TYPE_SFIXED64 => DynamicValue::Primitive(DynamicPrimitiveValue::Int64(0)),
            TYPE_SINT32 => DynamicValue::Primitive(DynamicPrimitiveValue::Int32(0)),
            TYPE_SINT64 => DynamicValue::Primitive(DynamicPrimitiveValue::Int64(0)),
        })
    }
}

impl protobuf_core::Message for DynamicMessage {
    fn type_url(&self) -> &'static str {
        todo!()
    }

    fn file_descriptor() -> &'static protobuf_core::StaticFileDescriptor
    where
        Self: Sized,
    {
        // Note possible to do statically.
        todo!()
    }

    fn parse(data: &[u8]) -> Result<Self>
    where
        Self: Sized,
    {
        // It's not possible for us to implement this as we don't have a descriptor.
        todo!()
    }

    fn parse_merge(&mut self, data: &[u8]) -> Result<()> {
        for wire_field in WireFieldIter::new(data) {
            let wire_field = wire_field?;

            let field_desc = match self.desc.field_by_number(wire_field.field_number) {
                Some(d) => d,
                None => return Err(err_msg("Unknown field")),
            };

            use FieldDescriptorProto_Type::*;

            let value = match field_desc.proto().typ() {
                TYPE_DOUBLE => DynamicValue::Primitive(DynamicPrimitiveValue::Double(
                    wire_field.parse_double()?,
                )),
                TYPE_FLOAT => {
                    DynamicValue::Primitive(DynamicPrimitiveValue::Float(wire_field.parse_float()?))
                }
                TYPE_INT64 => {
                    DynamicValue::Primitive(DynamicPrimitiveValue::Int64(wire_field.parse_int64()?))
                }
                TYPE_UINT64 => DynamicValue::Primitive(DynamicPrimitiveValue::Uint64(
                    wire_field.parse_uint64()?,
                )),
                TYPE_INT32 => {
                    DynamicValue::Primitive(DynamicPrimitiveValue::Int32(wire_field.parse_int32()?))
                }
                TYPE_FIXED64 => DynamicValue::Primitive(DynamicPrimitiveValue::Uint64(
                    wire_field.parse_fixed64()?,
                )),
                TYPE_FIXED32 => DynamicValue::Primitive(DynamicPrimitiveValue::Uint32(
                    wire_field.parse_fixed32()?,
                )),
                TYPE_BOOL => {
                    DynamicValue::Primitive(DynamicPrimitiveValue::Bool(wire_field.parse_bool()?))
                }
                TYPE_STRING => DynamicValue::Primitive(DynamicPrimitiveValue::String(
                    wire_field.parse_string()?,
                )),
                TYPE_GROUP => {
                    todo!()
                }
                TYPE_MESSAGE | TYPE_ENUM => match field_desc.find_type() {
                    Some(TypeDescriptor::Message(m)) => {
                        let mut val = DynamicMessage::new(m);
                        wire_field.parse_message_into(&mut val)?;
                        DynamicValue::Message(val)
                    }
                    Some(TypeDescriptor::Enum(e)) => {
                        let mut val = DynamicEnum::new(e);
                        wire_field.parse_enum_into(&mut val)?;
                        DynamicValue::Enum(val)
                    }
                    _ => {
                        return Err(format_err!(
                            "Unknown type while parsing: {:?}",
                            field_desc.proto()
                        ))
                    }
                },
                TYPE_BYTES => {
                    DynamicValue::Primitive(DynamicPrimitiveValue::Bytes(wire_field.parse_bytes()?))
                }
                TYPE_UINT32 => DynamicValue::Primitive(DynamicPrimitiveValue::Uint32(
                    wire_field.parse_uint32()?,
                )),
                TYPE_SFIXED32 => DynamicValue::Primitive(DynamicPrimitiveValue::Int32(
                    wire_field.parse_sfixed32()?,
                )),
                TYPE_SFIXED64 => DynamicValue::Primitive(DynamicPrimitiveValue::Int64(
                    wire_field.parse_sfixed64()?,
                )),
                TYPE_SINT32 => DynamicValue::Primitive(DynamicPrimitiveValue::Int32(
                    wire_field.parse_sint32()?,
                )),
                TYPE_SINT64 => DynamicValue::Primitive(DynamicPrimitiveValue::Int64(
                    wire_field.parse_sint64()?,
                )),
            };

            let is_repeated =
                field_desc.proto().label() == FieldDescriptorProto_Label::LABEL_REPEATED;

            if !is_repeated {
                self.fields
                    .insert(wire_field.field_number, DynamicField::Singular(value));
                continue;
            }

            // TODO: Only generate if needed.
            let default_value = Self::default_value_for_field(&field_desc)?;

            let mut existing_field =
                self.fields
                    .entry(wire_field.field_number)
                    .or_insert_with(|| {
                        DynamicField::Repeated(DynamicRepeatedField {
                            default_value,
                            values: vec![],
                            desc: field_desc,
                        })
                    });

            let existing_values = match existing_field {
                DynamicField::Repeated(v) => &mut v.values,
                _ => panic!(),
            };

            existing_values.push(value);
        }

        Ok(())
    }

    fn serialize(&self) -> Result<Vec<u8>> {
        // TODO: Go in field number order.
        // TODO: Ignore fields with default values (by using the sparse serializers).

        let mut out = vec![];

        for (field_num, field) in &self.fields {
            let (repeated, values) = match field {
                DynamicField::Singular(v) => (false, std::slice::from_ref(v)),
                DynamicField::Repeated(v) => (true, &v.values[..]),
            };

            for value in values {
                match value {
                    DynamicValue::Primitive(v) => {
                        // TODO: Choose sparse variations if not repeated.
                        match &v {
                            DynamicPrimitiveValue::Double(v) => {
                                WireField::serialize_double(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Float(v) => {
                                WireField::serialize_float(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Int32(v) => {
                                WireField::serialize_int32(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Int64(v) => {
                                WireField::serialize_int64(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Uint32(v) => {
                                WireField::serialize_uint32(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Uint64(v) => {
                                WireField::serialize_uint64(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Sint32(v) => {
                                WireField::serialize_sint32(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Sint64(v) => {
                                WireField::serialize_sint64(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Fixed32(v) => {
                                WireField::serialize_fixed32(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Fixed64(v) => {
                                WireField::serialize_fixed64(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Sfixed32(v) => {
                                WireField::serialize_sfixed32(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Sfixed64(v) => {
                                WireField::serialize_sfixed64(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::Bool(v) => {
                                WireField::serialize_bool(*field_num, *v, &mut out)
                            }
                            DynamicPrimitiveValue::String(v) => {
                                WireField::serialize_string(*field_num, v.as_ref(), &mut out)
                            }
                            DynamicPrimitiveValue::Bytes(v) => {
                                WireField::serialize_bytes(*field_num, v.as_ref(), &mut out)
                            }
                        }
                    }
                    DynamicValue::Enum(v) => {
                        if repeated {
                            WireField::serialize_enum(*field_num, v, &mut out)
                        } else {
                            WireField::serialize_sparse_enum(*field_num, v, &mut out)
                        }
                    }
                    DynamicValue::Message(v) => {
                        WireField::serialize_message(*field_num, v, &mut out)
                    }
                }?;
            }
        }

        Ok(out)
    }
}

impl protobuf_core::MessageReflection for DynamicMessage {
    fn fields(&self) -> &[protobuf_core::FieldDescriptorShort] {
        &self.desc.fields()
    }

    fn field_by_number(&self, num: FieldNumber) -> Option<Reflection> {
        self.fields.get(&num).and_then(|f| f.reflect())
    }

    fn field_by_number_mut<'a>(&'a mut self, num: FieldNumber) -> Option<ReflectionMut<'a>> {
        if !self.fields.contains_key(&num) {
            let field_desc = match self.desc.field_by_number(num) {
                Some(v) => v,
                None => return None,
            };

            use FieldDescriptorProto_Type::*;

            let default_value = match Self::default_value_for_field(&field_desc) {
                Ok(v) => v,
                Err(_) => return None,
            };

            let is_repeated =
                field_desc.proto().label() == FieldDescriptorProto_Label::LABEL_REPEATED;

            let default_field = if is_repeated {
                DynamicField::Repeated(DynamicRepeatedField {
                    values: vec![],
                    default_value,
                    desc: field_desc,
                })
            } else {
                DynamicField::Singular(default_value)
            };

            self.fields.insert(num, default_field);
        }

        self.fields.get_mut(&num).map(|f| f.reflect_mut())
    }

    fn field_number_by_name(&self, name: &str) -> Option<FieldNumber> {
        self.desc.field_number_by_name(name)
    }
}

#[derive(Clone)]
enum DynamicField {
    Singular(DynamicValue),
    Repeated(DynamicRepeatedField),
}

impl Reflect for DynamicField {
    fn reflect(&self) -> Option<Reflection> {
        match self {
            DynamicField::Singular(v) => v.reflect(),
            DynamicField::Repeated(v) => Some(Reflection::Repeated(v)),
        }
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        match self {
            DynamicField::Singular(v) => v.reflect_mut(),
            DynamicField::Repeated(v) => ReflectionMut::Repeated(v),
        }
    }
}

#[derive(Clone)]
enum DynamicValue {
    Primitive(DynamicPrimitiveValue),
    Enum(DynamicEnum),
    Message(DynamicMessage),
}

impl Reflect for DynamicValue {
    fn reflect(&self) -> Option<Reflection> {
        match self {
            DynamicValue::Primitive(v) => v.reflect(),
            DynamicValue::Enum(v) => Some(Reflection::Enum(v)),
            DynamicValue::Message(v) => Some(Reflection::Message(v)),
        }
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        match self {
            DynamicValue::Primitive(v) => v.reflect_mut(),
            DynamicValue::Enum(v) => ReflectionMut::Enum(v),
            DynamicValue::Message(v) => ReflectionMut::Message(v),
        }
    }
}

#[derive(Clone)]
struct DynamicRepeatedField {
    values: Vec<DynamicValue>,
    default_value: DynamicValue,
    desc: FieldDescriptor,
}

impl RepeatedFieldReflection for DynamicRepeatedField {
    fn len(&self) -> usize {
        self.values.len()
    }

    fn get(&self, index: usize) -> Option<Reflection> {
        self.values.get(index).and_then(|v| v.reflect())
    }

    fn get_mut(&mut self, index: usize) -> Option<ReflectionMut> {
        self.values.get_mut(index).map(|v| v.reflect_mut())
    }

    fn add(&mut self) -> ReflectionMut {
        self.values.push(self.default_value.clone());
        self.values.last_mut().unwrap().reflect_mut()
    }
}

#[derive(Clone)]
struct DynamicEnum {
    value: EnumValue,
    desc: EnumDescriptor,
}

impl DynamicEnum {
    pub fn new(desc: EnumDescriptor) -> Self {
        // TODO: Need to have better comprehension of the default value.
        Self { value: 0, desc }
    }
}

impl protobuf_core::Enum for DynamicEnum {
    fn parse(v: EnumValue) -> Result<Self>
    where
        Self: Sized,
    {
        // Can't implement without a descriptor.
        todo!()
    }

    fn parse_name(name: &str) -> Result<Self>
    where
        Self: Sized,
    {
        // Can't implement without a descriptor.
        todo!()
    }

    fn name(&self) -> &str {
        for val in self.desc.proto().value() {
            if val.number() == self.value {
                return val.name();
            }
        }

        "UNKNOWN"
    }

    fn value(&self) -> EnumValue {
        self.value
    }

    fn assign(&mut self, v: EnumValue) -> Result<()> {
        for val in self.desc.proto().value() {
            if val.number() == v {
                self.value = v;
                return Ok(());
            }
        }

        Err(err_msg("Unknown enum value"))
    }

    fn assign_name(&mut self, name: &str) -> Result<()> {
        for val in self.desc.proto().value() {
            if val.name() == name {
                self.value = val.number();
                return Ok(());
            }
        }

        Err(err_msg("Unknown enum value name"))
    }
}

#[derive(Clone)]
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
        match self {
            DynamicPrimitiveValue::Double(v) => ReflectionMut::F64(v),
            DynamicPrimitiveValue::Float(v) => ReflectionMut::F32(v),
            DynamicPrimitiveValue::Int32(v) => ReflectionMut::I32(v),
            DynamicPrimitiveValue::Int64(v) => ReflectionMut::I64(v),
            DynamicPrimitiveValue::Uint32(v) => ReflectionMut::U32(v),
            DynamicPrimitiveValue::Uint64(v) => ReflectionMut::U64(v),
            DynamicPrimitiveValue::Sint32(v) => ReflectionMut::I32(v),
            DynamicPrimitiveValue::Sint64(v) => ReflectionMut::I64(v),
            DynamicPrimitiveValue::Fixed32(v) => ReflectionMut::U32(v),
            DynamicPrimitiveValue::Fixed64(v) => ReflectionMut::U64(v),
            DynamicPrimitiveValue::Sfixed32(v) => ReflectionMut::I32(v),
            DynamicPrimitiveValue::Sfixed64(v) => ReflectionMut::I64(v),
            DynamicPrimitiveValue::Bool(v) => ReflectionMut::Bool(v),
            DynamicPrimitiveValue::String(v) => ReflectionMut::String(v),
            DynamicPrimitiveValue::Bytes(v) => ReflectionMut::Bytes(&mut v.0),
        }
    }
}
