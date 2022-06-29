use alloc::string::String;
use alloc::vec::Vec;
use std::cmp::PartialEq;
use std::collections::HashMap;

use common::errors::*;
use common::list::Appendable;
use protobuf_compiler::spec::Syntax;
use protobuf_core::reflection::RepeatedFieldReflection;
use protobuf_core::wire::{WireField, WireError, WireFieldIter};
use protobuf_core::{EnumValue, WireResult, FieldNumber};
use protobuf_core::codecs::*;
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

    // TODO: Move this to DynamicValue.
    fn default_value_for_field(field_desc: &FieldDescriptor) -> WireResult<DynamicValue> {
        use FieldDescriptorProto_Type::*;
        Ok(match field_desc.proto().typ() {
            TYPE_DOUBLE => DynamicValue::Primitive(DynamicPrimitiveValue::Double(0.0)),
            TYPE_FLOAT => DynamicValue::Primitive(DynamicPrimitiveValue::Float(0.0)),
            TYPE_INT64 => DynamicValue::Primitive(DynamicPrimitiveValue::Int64(0)),
            TYPE_UINT64 => DynamicValue::Primitive(DynamicPrimitiveValue::UInt64(0)),
            TYPE_INT32 => DynamicValue::Primitive(DynamicPrimitiveValue::Int32(0)),
            TYPE_FIXED64 => DynamicValue::Primitive(DynamicPrimitiveValue::UInt64(0)),
            TYPE_FIXED32 => DynamicValue::Primitive(DynamicPrimitiveValue::UInt32(0)),
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
                _ => return Err(WireError::BadDescriptor /* err_msg("Unknown type in descriptor")*/),
            },
            TYPE_BYTES => DynamicValue::Primitive(DynamicPrimitiveValue::Bytes(Vec::new().into())),
            TYPE_UINT32 => DynamicValue::Primitive(DynamicPrimitiveValue::UInt32(0)),
            TYPE_SFIXED32 => DynamicValue::Primitive(DynamicPrimitiveValue::Int32(0)),
            TYPE_SFIXED64 => DynamicValue::Primitive(DynamicPrimitiveValue::Int64(0)),
            TYPE_SINT32 => DynamicValue::Primitive(DynamicPrimitiveValue::Int32(0)),
            TYPE_SINT64 => DynamicValue::Primitive(DynamicPrimitiveValue::Int64(0)),
        })
    }
}

impl PartialEq for DynamicMessage {
    fn eq(&self, other: &Self) -> bool {
        // TODO: Check that the type URLs are equal
        self.fields == other.fields
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
        panic!()
    }

    fn parse(data: &[u8]) -> WireResult<Self>
    where
        Self: Sized,
    {
        // It's not possible for us to implement this as we don't have a descriptor.
        panic!()
    }

    fn parse_merge(&mut self, data: &[u8]) -> WireResult<()> {
        for wire_field in WireFieldIter::new(data) {
            let wire_field = wire_field?;

            let field_desc = match self.desc.field_by_number(wire_field.field_number) {
                Some(d) => d,
                // TODO: Check this behavior.
                None => continue //return Err(err_msg("Unknown field")),
            };

            let is_repeated =
                field_desc.proto().label() == FieldDescriptorProto_Label::LABEL_REPEATED;

            if !is_repeated {
                let value = DynamicValue::parse_singular_value(&field_desc, &wire_field)?;
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
                            desc: field_desc.clone(),
                        })
                    });

            let existing_values = match existing_field {
                DynamicField::Repeated(v) => &mut v.values,
                _ => panic!(),
            };

            DynamicValue::parse_repeated_values(&field_desc, &wire_field, existing_values)?;
        }

        Ok(())
    }

    fn serialize_to<A: Appendable<Item = u8>>(&self, out: &mut A) -> Result<()> {
        // TODO: Go in field number order.
        // TODO: Ignore fields with default values in proto3 (by using the sparse
        // serializers).

        let mut out = vec![];

        for (field_num, field) in &self.fields {
            let (repeated, values) = match field {
                DynamicField::Singular(v) => (false, std::slice::from_ref(v)),
                DynamicField::Repeated(v) => (true, &v.values[..]),
            };

            for value in values {
                // TODO: NEed an alternative form for repeated values.
                value.serialize_to(*field_num, &mut out)?;
            }
        }

        Ok(())
    }
}

impl protobuf_core::MessageReflection for DynamicMessage {
    fn fields(&self) -> &[protobuf_core::FieldDescriptorShort] {
        &self.desc.fields()
    }

    fn field_by_number(&self, num: FieldNumber) -> Option<Reflection> {
        let field = match self.fields.get(&num) {
            Some(v) => v,
            None => return None,
        };

        // Check field presence.
        let present = match self.desc.syntax() {
            Syntax::Proto2 => {
                // Nothing else to check.
                // Presence of the value in the map is good enough.
                true
            }
            Syntax::Proto3 => match field {
                DynamicField::Singular(v) => {
                    let field_desc = self.desc.field_by_number(num).unwrap();
                    if field_desc.proto().has_oneof_index() {
                        true
                    } else {
                        let default_value = Self::default_value_for_field(&field_desc).unwrap();

                        *v != default_value
                    }
                }
                DynamicField::Repeated(v) => true,
            },
        };

        if !present {
            return None;
        }

        Some(field.reflect())
    }

    fn field_by_number_mut<'a>(&'a mut self, num: FieldNumber) -> Option<ReflectionMut<'a>> {
        // TODO: Mutating a oneof field should clear all of the other ones.

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

#[derive(Clone, PartialEq)]
enum DynamicField {
    Singular(DynamicValue),
    Repeated(DynamicRepeatedField),
}

impl Reflect for DynamicField {
    fn reflect(&self) -> Reflection {
        match self {
            DynamicField::Singular(v) => v.reflect(),
            DynamicField::Repeated(v) => Reflection::Repeated(v),
        }
    }

    fn reflect_mut(&mut self) -> ReflectionMut {
        match self {
            DynamicField::Singular(v) => v.reflect_mut(),
            DynamicField::Repeated(v) => ReflectionMut::Repeated(v),
        }
    }
}

#[derive(Clone, PartialEq)]
enum DynamicValue {
    Primitive(DynamicPrimitiveValue),
    Enum(DynamicEnum),
    Message(DynamicMessage),
}

impl Reflect for DynamicValue {
    fn reflect(&self) -> Reflection {
        match self {
            DynamicValue::Primitive(v) => v.reflect(),
            DynamicValue::Enum(v) => Reflection::Enum(v),
            DynamicValue::Message(v) => Reflection::Message(v),
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

macro_rules! define_primitive_values {
    ($v:ident, $( $name:ident ( $t:ty ) $proto_type:ident => $reflection_variant:ident ( $reflection_value:expr, $reflection_mut:expr, $serialize_value:expr ) ),*) => {
        impl DynamicValue {
            fn parse_singular_value(field_desc: &FieldDescriptor, wire_field: &WireField) -> WireResult<DynamicValue> {
                use FieldDescriptorProto_Type::*;
        
                Ok(match field_desc.proto().typ() {
                    TYPE_GROUP => {
                        todo!()
                    }
                    TYPE_MESSAGE | TYPE_ENUM => match field_desc.find_type() {
                        Some(TypeDescriptor::Message(m)) => {
                            let mut val = DynamicMessage::new(m);
                            MessageCodec::parse_into(wire_field, &mut val)?;
                            DynamicValue::Message(val)
                        }
                        Some(TypeDescriptor::Enum(e)) => {
                            let mut val = DynamicEnum::new(e);
                            EnumCodec::parse_into(wire_field, &mut val)?;
                            DynamicValue::Enum(val)
                        }
                        _ => {
                            return Err(WireError::BadDescriptor);
                            // return Err(format_err!(
                            //     "Unknown type while parsing: {:?}",
                            //     field_desc.proto()
                            // ))
                        }
                    },
                    $(
                        $proto_type => DynamicValue::Primitive(DynamicPrimitiveValue::$name(
                            <concat_idents!($name, Codec)>::parse(wire_field)?
                        ))
                    ),*
                })
            }

            // TODO: Directly write to the output vector of the caller.
            fn parse_repeated_values(field_desc: &FieldDescriptor, wire_field: &WireField, values: &mut Vec<DynamicValue>) -> WireResult<()> {
                use FieldDescriptorProto_Type::*;

                match field_desc.proto().typ() {
                    TYPE_INT64 => {
                        for v in wire_field.parse_repeated_int64() {
                            values.push(DynamicValue::Primitive(DynamicPrimitiveValue::Int64(v?)));
                        }

                        return Ok(());
                    }
                    TYPE_UINT64 => {
                        for v in wire_field.parse_repeated_uint64() {
                            values.push(DynamicValue::Primitive(DynamicPrimitiveValue::UInt64(v?)));
                        }

                        return Ok(());
                    },

                    // TODO: Add all other supported packable types.

                    // Other types don't support packing.
                    _ => {}
                };

                // Fallback to types that can't be packed.
                values.push(Self::parse_singular_value(field_desc, wire_field)?);
                Ok(())
            }

            fn serialize_to<A: Appendable<Item = u8>>(&self, field_num: FieldNumber, out: &mut A) -> Result<()> {
                match self {
                    DynamicValue::Primitive(v) => {
                        // TODO: Choose sparse variations if not repeated.
                        match v {
                            $(
                                DynamicPrimitiveValue::$name($v) => {
                                    <concat_idents!($name, Codec)>::serialize(field_num, $serialize_value, out)
                                }
                            ),*
                        }?
                    }
                    DynamicValue::Enum(v) => {
                        // if repeated {
                        EnumCodec::serialize(field_num, v, out)?
                            // WireField::serialize_enum(*field_num, v, &mut out)?
                        // } else {
                        //     WireField::serialize_sparse_enum(field_num, v, &mut out)?
                        // }
                    }
                    DynamicValue::Message(v) => {
                        MessageCodec::serialize(field_num, v, out)?
                    }
                };
                Ok(())
            }
        }

        #[derive(Clone, PartialEq)]
        pub(crate) enum DynamicPrimitiveValue {
            $( $name($t) ),*
        }

        impl Reflect for DynamicPrimitiveValue {
            fn reflect(&self) -> Reflection {
                match self {
                    $( DynamicPrimitiveValue::$name($v) => Reflection::$reflection_variant($reflection_value) ),*
                }
            }
        
            fn reflect_mut(&mut self) -> ReflectionMut {
                match self {
                    $( DynamicPrimitiveValue::$name($v) => ReflectionMut::$reflection_variant($reflection_mut) ),*
                }
            }
        }
    };
}

define_primitive_values!(
    v,
    Double(f64) TYPE_DOUBLE => F64(v, v, *v),
    Float(f32) TYPE_FLOAT => F32(v, v, *v),
    Int32(i32) TYPE_INT32 => I32(v, v, *v),
    Int64(i64) TYPE_INT64 => I64(v, v, *v),
    UInt32(u32) TYPE_UINT32 => U32(v, v, *v),
    UInt64(u64) TYPE_UINT64 => U64(v, v, *v),
    SInt32(i32) TYPE_SINT32 => I32(v, v, *v),
    SInt64(i64) TYPE_SINT64 => I64(v, v, *v),
    Fixed32(u32) TYPE_FIXED32 => U32(v, v, *v),
    Fixed64(u64) TYPE_FIXED64 => U64(v, v, *v),
    SFixed32(i32) TYPE_SFIXED32 => I32(v, v, *v),
    SFixed64(i64) TYPE_SFIXED64 => I64(v, v, *v),
    Bool(bool) TYPE_BOOL => Bool(v, v, *v),
    String(String) TYPE_STRING => String(v, v, v.as_ref()),
    Bytes(BytesField) TYPE_BYTES => Bytes(v.as_ref(), &mut v.0, &*v)
);


#[derive(Clone)]
struct DynamicRepeatedField {
    values: Vec<DynamicValue>,
    default_value: DynamicValue,
    desc: FieldDescriptor,
}

impl PartialEq for DynamicRepeatedField {
    fn eq(&self, other: &Self) -> bool {
        self.values == other.values
    }
}

impl RepeatedFieldReflection for DynamicRepeatedField {
    fn len(&self) -> usize {
        self.values.len()
    }

    fn get(&self, index: usize) -> Option<Reflection> {
        self.values.get(index).map(|v| v.reflect())
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

impl PartialEq for DynamicEnum {
    fn eq(&self, other: &Self) -> bool {
        // TODO: Check the type URL
        self.value == other.value
    }
}

impl protobuf_core::Enum for DynamicEnum {
    fn parse(v: EnumValue) -> WireResult<Self>
    where
        Self: Sized,
    {
        // Can't implement without a descriptor.
        todo!()
    }

    fn parse_name(name: &str) -> WireResult<Self>
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

    fn assign(&mut self, v: EnumValue) -> WireResult<()> {
        for val in self.desc.proto().value() {
            if val.number() == v {
                self.value = v;
                return Ok(());
            }
        }

        Err(WireError::UnknownEnumVariant)
    }

    fn assign_name(&mut self, name: &str) -> WireResult<()> {
        for val in self.desc.proto().value() {
            if val.name() == name {
                self.value = val.number();
                return Ok(());
            }
        }

        Err(WireError::UnknownEnumVariant)
    }
}

