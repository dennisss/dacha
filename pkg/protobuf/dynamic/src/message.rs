use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use common::hash::SumHasherBuilder;
use protobuf_core::unknown::UnknownFieldSet;
use std::cmp::PartialEq;
use std::collections::HashMap;

use common::errors::*;
use common::list::Appendable;
use protobuf_core::reflection::RepeatedFieldReflection;
use protobuf_core::reflection::{Reflect, Reflection, ReflectionMut};
use protobuf_core::wire::{WireError, WireField, WireFieldIter};
use protobuf_core::BytesField;
use protobuf_core::{codecs::*, ExtensionSet};
use protobuf_core::{EnumValue, FieldNumber, WireResult};
use protobuf_core::{RepeatedValues, SingularValue, Value};
use protobuf_descriptor::{FieldDescriptorProto_Label, FieldDescriptorProto_Type};

use crate::descriptor_pool::{EnumDescriptor, FieldDescriptor, MessageDescriptor, TypeDescriptor};
use crate::spec::Syntax;

#[derive(Clone, PartialEq)]
pub struct DynamicMessage {
    // NOTE: Field numbers are frequently sequential and pretty easy to hash.
    // TODO: We should just be able to make this a flat Vec based on the order in the field
    // descriptor (have on shared hash map)
    fields: HashMap<FieldNumber, Value, SumHasherBuilder>,

    extensions: ExtensionSet,

    desc: MessageDescriptor,
}

impl DynamicMessage {
    pub fn new(desc: MessageDescriptor) -> Self {
        Self {
            fields: HashMap::with_hasher(SumHasherBuilder::default()),
            extensions: ExtensionSet::default(),
            desc,
        }
    }

    // TODO: Return a 'Value' here
    pub(crate) fn default_value_for_field(
        field_desc: &FieldDescriptor,
    ) -> WireResult<SingularValue> {
        use FieldDescriptorProto_Type::*;
        Ok(match field_desc.proto().typ() {
            TYPE_DOUBLE => SingularValue::Double(0.0),
            TYPE_FLOAT => SingularValue::Float(0.0),
            TYPE_INT64 => SingularValue::Int64(0),
            TYPE_UINT64 => SingularValue::UInt64(0),
            TYPE_INT32 => SingularValue::Int32(0),
            TYPE_FIXED64 => SingularValue::UInt64(0),
            TYPE_FIXED32 => SingularValue::UInt32(0),
            TYPE_BOOL => SingularValue::Bool(false),
            TYPE_STRING => SingularValue::String(String::new()),
            TYPE_GROUP => {
                todo!()
            }
            TYPE_MESSAGE | TYPE_ENUM => match field_desc.find_type() {
                Some(TypeDescriptor::Message(m)) => {
                    let val = DynamicMessage::new(m);
                    SingularValue::Message(Box::new(val))
                }
                Some(TypeDescriptor::Enum(e)) => {
                    let val = DynamicEnum::new(e);
                    SingularValue::Enum(Box::new(val))
                }
                _ => {
                    return Err(
                        WireError::BadDescriptor, /* err_msg("Unknown type in descriptor") */
                    );
                }
            },
            TYPE_BYTES => SingularValue::Bytes(Vec::new().into()),
            TYPE_UINT32 => SingularValue::UInt32(0),
            TYPE_SFIXED32 => SingularValue::Int32(0),
            TYPE_SFIXED64 => SingularValue::Int64(0),
            TYPE_SINT32 => SingularValue::Int32(0),
            TYPE_SINT64 => SingularValue::Int64(0),
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

            let field_desc = match self.desc.field_by_number(wire_field.field.field_number) {
                Some(d) => d,
                // TODO: Check this behavior.
                // TODO: Add to unknown fields in this case.
                None => {
                    self.extensions.parse_merge(wire_field.span.into())?;
                    continue;
                }
            };

            let is_repeated =
                field_desc.proto().label() == FieldDescriptorProto_Label::LABEL_REPEATED;

            // TODO: Only generate if needed.
            let default_value = Self::default_value_for_field(&field_desc)?;

            let mut existing_field = self
                .fields
                .entry(wire_field.field.field_number)
                .or_insert_with(|| Value::new(default_value, is_repeated));

            existing_field.parse_merge(&wire_field.field)?;
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
            field.serialize_to(*field_num, out)?;
        }

        self.extensions.serialize_to(out)?;

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
        &self.desc.fields_short()
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
                Value::Singular(v) => {
                    let field_desc = self.desc.field_by_number(num).unwrap();
                    if field_desc.proto().has_oneof_index() {
                        true
                    } else {
                        let default_value = Self::default_value_for_field(&field_desc).unwrap();

                        *v != default_value
                    }
                }
                Value::Repeated(v) => true,
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

            let default_field = Value::new(default_value, is_repeated);

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

    fn unknown_fields(&self) -> Option<&UnknownFieldSet> {
        Some(self.extensions.unknown_fields())
    }

    fn extensions(&self) -> Option<&ExtensionSet> {
        Some(&self.extensions)
    }

    fn extensions_mut(&mut self) -> Option<&mut ExtensionSet> {
        Some(&mut self.extensions)
    }
}

// TODO: Make fully private
#[derive(Clone)]
pub(crate) struct DynamicEnum {
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
