use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use common::hash::SumHasherBuilder;
use std::cmp::PartialEq;
use std::collections::HashMap;

use common::errors::*;
use common::list::Appendable;
use protobuf_compiler::spec::Syntax;
use protobuf_core::codecs::*;
use protobuf_core::reflection::RepeatedFieldReflection;
use protobuf_core::wire::{WireError, WireField, WireFieldIter};
use protobuf_core::{EnumValue, FieldNumber, WireResult};
use protobuf_descriptor::{FieldDescriptorProto_Label, FieldDescriptorProto_Type};

use crate::descriptor_pool::{EnumDescriptor, FieldDescriptor, MessageDescriptor, TypeDescriptor};
use crate::reflection::{Reflect, Reflection, ReflectionMut};
use crate::BytesField;

#[derive(Clone)]
pub struct DynamicMessage {
    // NOTE: Field numbers are frequently sequential and pretty easy to hash.
    fields: HashMap<FieldNumber, DynamicField, SumHasherBuilder>,
    desc: MessageDescriptor,
}

impl DynamicMessage {
    pub fn new(desc: MessageDescriptor) -> Self {
        Self {
            fields: HashMap::with_hasher(SumHasherBuilder::default()),
            desc,
        }
    }

    // TODO: Move this to DynamicValue.
    fn default_value_for_field(field_desc: &FieldDescriptor) -> WireResult<DynamicValue> {
        use FieldDescriptorProto_Type::*;
        Ok(match field_desc.proto().typ() {
            TYPE_DOUBLE => DynamicValue::Double(0.0),
            TYPE_FLOAT => DynamicValue::Float(0.0),
            TYPE_INT64 => DynamicValue::Int64(0),
            TYPE_UINT64 => DynamicValue::UInt64(0),
            TYPE_INT32 => DynamicValue::Int32(0),
            TYPE_FIXED64 => DynamicValue::UInt64(0),
            TYPE_FIXED32 => DynamicValue::UInt32(0),
            TYPE_BOOL => DynamicValue::Bool(false),
            TYPE_STRING => DynamicValue::String(String::new()),
            TYPE_GROUP => {
                todo!()
            }
            TYPE_MESSAGE | TYPE_ENUM => match field_desc.find_type() {
                Some(TypeDescriptor::Message(m)) => {
                    let val = DynamicMessage::new(m);
                    DynamicValue::Message(Box::new(val))
                }
                Some(TypeDescriptor::Enum(e)) => {
                    let val = DynamicEnum::new(e);
                    DynamicValue::Enum(Box::new(val))
                }
                _ => {
                    return Err(
                        WireError::BadDescriptor, /* err_msg("Unknown type in descriptor") */
                    );
                }
            },
            TYPE_BYTES => DynamicValue::Bytes(Vec::new().into()),
            TYPE_UINT32 => DynamicValue::UInt32(0),
            TYPE_SFIXED32 => DynamicValue::Int32(0),
            TYPE_SFIXED64 => DynamicValue::Int64(0),
            TYPE_SINT32 => DynamicValue::Int32(0),
            TYPE_SINT64 => DynamicValue::Int64(0),
        })
    }
}

impl protobuf_core::Message for DynamicMessage {
    fn type_url(&self) -> &str {
        self.desc.type_url()
    }

    fn parse_merge(&mut self, data: &[u8]) -> WireResult<()> {
        for wire_field in WireFieldIter::new(data) {
            let wire_field = wire_field?;

            let field_desc = match self.desc.field_by_number(wire_field.field_number) {
                Some(d) => d,
                // TODO: Check this behavior.
                None => continue, //return Err(err_msg("Unknown field")),
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
                        DynamicField::Repeated(DynamicRepeatedValues::new(default_value))
                    });

            let existing_values = match existing_field {
                DynamicField::Repeated(ref mut v) => v,
                _ => panic!(),
            };

            existing_values.parse_merge(&wire_field)?;
        }

        Ok(())
    }

    fn serialize(&self) -> Result<Vec<u8>> {
        let mut out = vec![];
        self.serialize_to(&mut out)?;
        Ok(out)
    }

    fn serialize_to<A: Appendable<Item = u8> + ?Sized>(&self, out: &mut A) -> Result<()> {
        // TODO: Go in field number order.
        // TODO: Ignore fields with default values in proto3 (by using the sparse
        // serializers).

        for (field_num, field) in &self.fields {
            match field {
                DynamicField::Singular(v) => {
                    v.serialize_singular_value(*field_num, out)?;
                }
                DynamicField::Repeated(v) => {
                    v.serialize(*field_num, out)?;
                }
            }
        }

        Ok(())
    }

    fn merge_from(&mut self, other: &Self) -> Result<()>
    where
        Self: Sized,
    {
        use protobuf_core::ReflectMergeFrom;
        self.reflect_merge_from(other)
    }

    fn box_clone(&self) -> Box<dyn protobuf_core::Message> {
        Box::new(self.clone())
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
                DynamicField::Repeated(DynamicRepeatedValues::new(default_value))
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

    fn box_clone2(&self) -> Box<dyn protobuf_core::MessageReflection + 'static> {
        Box::new(self.clone())
    }
}

#[derive(Clone)]
enum DynamicField {
    Singular(DynamicValue),
    Repeated(DynamicRepeatedValues),
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

macro_rules! define_primitive_values {
    ($v:ident, $( $name:ident ( $t:ty ) $proto_type:ident => $reflection_variant:ident ( $reflection_value:expr, $reflection_mut:expr, $serialize_value:expr ) ),*) => {

        enum DynamicValue {
            Enum(Box<dyn protobuf_core::Enum>),
            Message(Box<dyn protobuf_core::MessageReflection>),
            $( $name($t) ),*
        }

        impl Clone for DynamicValue {
            fn clone(&self) -> Self {
                match self {
                    DynamicValue::Enum(v) => DynamicValue::Enum(v.box_clone()),
                    DynamicValue::Message(v) => DynamicValue::Message(protobuf_core::MessageReflection::box_clone2(v.as_ref())),
                    $( DynamicValue::$name(v) => DynamicValue::$name(v.clone()) ),*
                }
            }
        }

        // TODO: Make sure that this is only ever used when checking if a singular field is equal to the default value (which only should happen for primitive values).
        impl PartialEq for DynamicValue {
            fn eq(&self, other: &Self) -> bool {
                match self {
                    DynamicValue::Enum(v) => {
                        // TODO: also match to integer types? (or at least enfore that types exactly match)
                        match other {
                            DynamicValue::Enum(v2) => v.value() == v2.value(),
                            _ => { false }
                        }
                    },
                    DynamicValue::Message(v) => todo!(),
                    $( DynamicValue::$name(v) => match other { DynamicValue::$name(v2) => { v == v2 } _ => { false } } ),*
                }
            }
        }

        impl Reflect for DynamicValue {
            fn reflect(&self) -> Reflection {
                match self {
                    DynamicValue::Enum(v) => Reflection::Enum(v.as_ref()),
                    DynamicValue::Message(v) => Reflection::Message(v.as_ref()),
                    $( DynamicValue::$name($v) => Reflection::$reflection_variant($reflection_value) ),*
                }
            }

            fn reflect_mut(&mut self) -> ReflectionMut {
                match self {
                    DynamicValue::Enum(v) => ReflectionMut::Enum(v.as_mut()),
                    DynamicValue::Message(v) => ReflectionMut::Message(v.as_mut()),
                    $( DynamicValue::$name($v) => ReflectionMut::$reflection_variant($reflection_mut) ),*
                }
            }
        }

        impl DynamicValue {
            fn parse_singular_value(field_desc: &FieldDescriptor, wire_field: &WireField) -> WireResult<DynamicValue> {
                use FieldDescriptorProto_Type::*;

                Ok(match field_desc.proto().typ() {
                    TYPE_GROUP => {
                        todo!()
                    }
                    // TODO: If the type is one that is already linked to the current binary, parse into a 'static' type instead of a dynamic one.
                    TYPE_MESSAGE | TYPE_ENUM => match field_desc.find_type() {
                        Some(TypeDescriptor::Message(m)) => {
                            let mut val = DynamicMessage::new(m);
                            MessageCodec::parse_into(wire_field, &mut val)?;
                            DynamicValue::Message(Box::new(val))
                        }
                        Some(TypeDescriptor::Enum(e)) => {
                            let mut val = DynamicEnum::new(e);
                            EnumCodec::parse_into(wire_field, &mut val)?;
                            DynamicValue::Enum(Box::new(val))
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
                        $proto_type => DynamicValue::$name(
                            <concat_idents!($name, Codec)>::parse(wire_field)?
                        )
                    ),*
                })
            }

            fn serialize_singular_value<A: Appendable<Item = u8> + ?Sized>(&self, field_num: FieldNumber, out: &mut A) -> Result<()> {
                match self {
                    $(
                    DynamicValue::$name($v) => {
                        <concat_idents!($name, Codec)>::serialize_sparse(field_num, $serialize_value, out)?
                    }
                    ),*
                    DynamicValue::Enum(v) => {
                        EnumCodec::serialize_sparse(field_num, v.as_ref(), out)?
                    }
                    DynamicValue::Message(v) => {
                        MessageCodec::serialize(field_num, v.as_ref(), out)?
                    }
                };
                Ok(())
            }
        }

        enum DynamicRepeatedValues {
            Enum {
                values: Vec<Box<dyn protobuf_core::Enum>>,

                /// Default value used when appending a new entry to this repeated field.
                default_value: Box<dyn protobuf_core::Enum>
            },
            Message {
                values: Vec<protobuf_core::MessagePtr<dyn protobuf_core::MessageReflection>>,

                /// Default value used when appending a new entry to this repeated field.
                default_value: Box<dyn protobuf_core::MessageReflection>
            },
            $( $name { values: Vec<$t>, default_value: $t, } ),*
        }

        impl Clone for DynamicRepeatedValues {
            fn clone(&self) -> Self {
                match self {
                    DynamicRepeatedValues::Message { values, default_value } => Self::Message {
                        values: values.iter().map(|v| protobuf_core::MessagePtr::new_boxed(v.box_clone2())).collect(),
                        default_value: protobuf_core::MessageReflection::box_clone2(default_value.as_ref())
                    },
                    DynamicRepeatedValues::Enum { values, default_value } => Self::Enum {
                        values: values.iter().map(|v| v.box_clone()).collect(),
                        default_value: default_value.box_clone()
                    },
                    $( DynamicRepeatedValues::$name { values, default_value } => Self::$name {
                        values: values.clone(),
                        default_value: default_value.clone()
                    } ),*
                }
            }
        }

        impl DynamicRepeatedValues {
            fn new(default_value: DynamicValue) -> Self {
                match default_value {
                    DynamicValue::Message(v) => Self::Message { values: vec![], default_value: v },
                    DynamicValue::Enum(v) => Self::Enum { values: vec![], default_value: v },
                    $( DynamicValue::$name(v) => Self::$name { values: vec![], default_value: v } ),*
                }
            }

            fn parse_merge(&mut self, wire_field: &WireField) -> WireResult<()> {
                match self {
                    DynamicRepeatedValues::Message { values, default_value }  => {
                        // NOTE: Doesn't support packed serialization.
                        let mut val = protobuf_core::MessageReflection::box_clone2(default_value.as_ref());
                        MessageCodec::parse_into(wire_field, val.as_mut())?;
                        values.push(protobuf_core::MessagePtr::new_boxed(val));
                    }
                    DynamicRepeatedValues::Enum { values, default_value } => {
                        for v in EnumCodec::parse_repeated::<AnonymousEnum>(wire_field) {
                            let v = v?;

                            let mut val = default_value.box_clone();
                            val.assign(v.value);
                            values.push(val);
                        }
                    }
                    $(
                        DynamicRepeatedValues::$name { values, .. } => {
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
                    DynamicRepeatedValues::Enum { values, .. } => {
                        EnumCodec::serialize_repeated_dyn(field_num, &values[..], out)?;
                    }
                    DynamicRepeatedValues::Message { values, .. } => {
                        MessageCodec::serialize_repeated(field_num, &values, out)?;
                    }
                    $(
                    DynamicRepeatedValues::$name { values, .. } => {
                        // $serialize_value
                        <concat_idents!($name, Codec)>::serialize_repeated(field_num, &values[..], out)?;
                    }
                    ),*
                };
                Ok(())
            }
        }

        /*
        impl PartialEq for DynamicRepeatedValues {
            fn eq(&self, other: &Self) -> bool {
                // Compare while ignoring the default value.

                match self {
                    DynamicRepeatedValues::Enum { values, .. } => {
                        if let DynamicRepeatedValues::Enum { values: other_values, .. } = other {
                            values == other_values
                        } else {
                            false
                        }
                    },
                    DynamicRepeatedValues::Message { values, .. } => {
                        if let DynamicRepeatedValues::Message { values: other_values, .. } = other {
                            values == other_values
                        } else {
                            false
                        }
                    },
                    $(
                        DynamicRepeatedValues::$name { values, .. } => {
                            if let DynamicRepeatedValues::$name { values: other_values, .. } = other {
                                values == other_values
                            } else {
                                false
                            }
                        }
                    ),*
                }
            }
        }
        */

        impl RepeatedFieldReflection for DynamicRepeatedValues {
            fn reflect_len(&self) -> usize {
                match self {
                    DynamicRepeatedValues::Enum { values, .. } => values.len(),
                    DynamicRepeatedValues::Message { values, .. } => values.len(),
                    $(
                        DynamicRepeatedValues::$name { values, .. } => values.len()
                    ),*
                }
            }

            fn reflect_get(&self, index: usize) -> Option<Reflection> {
                match self {
                    DynamicRepeatedValues::Enum { values, .. } => {
                        values.get(index).map(|v| Reflection::Enum(v.as_ref()))
                    },
                    DynamicRepeatedValues::Message { values, .. } => {
                        values.get(index).map(|v| Reflection::Message(v.as_ref()))
                    },
                    $(
                        DynamicRepeatedValues::$name { values, .. } => {
                            values.get(index).map(|$v| Reflection::$reflection_variant($reflection_value))
                        }
                    ),*
                }
            }

            fn reflect_get_mut(&mut self, index: usize) -> Option<ReflectionMut> {
                match self {
                    DynamicRepeatedValues::Enum { values, .. } => {
                        values.get_mut(index).map(|v| ReflectionMut::Enum(v.as_mut()))
                    },
                    DynamicRepeatedValues::Message { values, .. } => {
                        values.get_mut(index).map(|v| ReflectionMut::Message(v.as_mut()))
                    },
                    $(
                        DynamicRepeatedValues::$name { values, .. } => {
                            values.get_mut(index).map(|$v| ReflectionMut::$reflection_variant($reflection_mut))
                        }
                    ),*
                }
            }

            fn reflect_add(&mut self) -> ReflectionMut {
                match self {
                    DynamicRepeatedValues::Enum { values, default_value } => {
                        values.push( default_value.box_clone());
                    },
                    DynamicRepeatedValues::Message { values, default_value } => {
                        values.push(protobuf_core::MessagePtr::new_boxed(default_value.box_clone2()));
                    },
                    $(
                        DynamicRepeatedValues::$name { values, default_value } => {
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

    fn box_clone(&self) -> Box<dyn protobuf_core::Enum> {
        Box::new(self.clone())
    }
}

#[derive(Clone)]
struct AnonymousEnum {
    value: EnumValue,
}

impl protobuf_core::Enum for AnonymousEnum {
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

    fn box_clone(&self) -> Box<dyn protobuf_core::Enum> {
        Box::new(self.clone())
    }
}
