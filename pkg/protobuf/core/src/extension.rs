use alloc::boxed::Box;
use common::bytes::Bytes;
use core::any::Any;
use core::ops::Deref;
use std::collections::HashMap;

use common::hash::SumHasherBuilder;
use common::list::Appendable;
use common::{const_default::ConstDefault, errors::*};

use crate::{
    types::ExtensionNumberType,
    unknown::UnknownFieldSet,
    wire::{WireField, WireFieldIter, WireValue},
    Enum, Message, SingularValue, StringPtr, Value, WireError, WireResult,
};

pub enum ExtensionRef<'a, T> {
    Pointer(&'a T),
    Owned(T),
    Boxed(Box<T>),
}

impl<'a, T> Deref for ExtensionRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Owned(v) => &v,
            Self::Pointer(v) => *v,
            Self::Boxed(v) => v.as_ref(),
        }
    }
}

pub trait ExtensionTag {
    /// Used for wire field parsing/serialization.
    fn extension_number(&self) -> ExtensionNumberType;

    /// Fully qualified name of the extension field (of the form
    /// [package].[optional_sub_messages].[field])
    ///
    /// Used for text proto parsing/printing.
    fn extension_name(&self) -> StringPtr;

    /// Used for parsing value.
    fn default_extension_value(&self) -> Value;
}

/*
pub trait StaticExtensionTag: ExtensionTag {
    type ExtensionType: 'static;

    fn downcast_extension_value<'a>(
        &self,
        value: ExtensionRef<'a, Value>,
    ) -> Option<ExtensionRef<'a, Self::ExtensionType>>;

    fn downcast_extension_value_mut<'a>(
        &self,
        value: &'a mut Value,
    ) -> Option<&'a mut Self::ExtensionType>;
}
*/

/// TODO: PartialEq of this must consider values in unknown_fields of the other
/// message.
#[derive(Default, Clone, PartialEq)]
pub struct ExtensionSet {
    extensions: Option<HashMap<ExtensionNumberType, Extension, SumHasherBuilder>>,
    unknown_fields: UnknownFieldSet,
}

impl ConstDefault for ExtensionSet {
    const DEFAULT: Self = Self {
        extensions: None,
        unknown_fields: UnknownFieldSet::DEFAULT,
    };
}

#[derive(Clone, PartialEq)]
pub struct Extension {
    pub value: Value,

    // This is mainly needed for text proto printing.
    // TODO: Eventually optimize this out.
    // (instead point to some field descriptor?)
    // TODO: Must ensure that messages don't mix between descriptor pools. Otherwise this may be
    // invalid.
    pub name: StringPtr,
}

impl ExtensionSet {
    pub fn unknown_fields(&self) -> &UnknownFieldSet {
        &self.unknown_fields
    }

    pub fn parse_merge(&mut self, wire_field: Bytes) -> WireResult<()> {
        if let Some(extensions) = &mut self.extensions {
            // TODO: Maybe merge into any existing extensions.
        }

        self.unknown_fields.fields.push(wire_field);

        Ok(())
    }

    pub fn serialize_to<A: Appendable<Item = u8> + ?Sized>(&self, out: &mut A) -> Result<()> {
        // TODO: Explicitly ignore any numbers that overlap with the defined extensions.
        self.unknown_fields.serialize_to(out)?;

        if let Some(extensions) = &self.extensions {
            for (num, ext) in extensions {
                ext.value.serialize_to(*num, out)?;
            }
        }

        Ok(())
    }

    pub fn contains(&self, tag: &dyn ExtensionTag) -> bool {
        if let Some(extensions) = &self.extensions {
            if extensions.contains_key(&tag.extension_number()) {
                return true;
            }
        }

        for field in &self.unknown_fields.fields {
            for wire_field in WireFieldIter::new(&field) {
                if let Ok(wire_field) = wire_field {
                    if wire_field.field.field_number == tag.extension_number() {
                        return true;
                    }
                }
            }
        }

        false
    }

    pub fn get_dynamic<'a>(
        &'a self,
        tag: &dyn ExtensionTag,
    ) -> WireResult<ExtensionRef<'a, Value>> {
        if let Some(extensions) = &self.extensions {
            if let Some(e) = extensions.get(&tag.extension_number()) {
                // Light protection against number collisions.
                if e.name != tag.extension_name() {
                    return Err(WireError::BadDescriptor);
                }

                return Ok(ExtensionRef::Pointer(&e.value));
            }
        }

        let mut value = tag.default_extension_value();
        for field in &self.unknown_fields.fields {
            for wire_field in WireFieldIter::new(&field) {
                let wire_field = wire_field?;
                if wire_field.field.field_number != tag.extension_number() {
                    continue;
                }

                value.parse_merge(&wire_field.field)?;
            }
        }

        Ok(ExtensionRef::Owned(value))
    }

    pub fn get_dynamic_mut<'a>(&'a mut self, tag: &dyn ExtensionTag) -> WireResult<&'a mut Value> {
        let extensions = self
            .extensions
            .get_or_insert_with(|| HashMap::with_hasher(SumHasherBuilder::default()));

        if !extensions.contains_key(&tag.extension_number()) {
            // TODO: Eventually we should register all extension types at
            // compile time and parse them during regular protobuf message
            // parsing (as that's what the regular protobuf compiler does).

            let mut value = tag.default_extension_value();

            let mut field_i = 0;
            while field_i < self.unknown_fields.fields.len() {
                let mut matched_field = false;

                // NOTE: This should only ever loop once.
                for wire_field in WireFieldIter::new(&self.unknown_fields.fields[field_i]) {
                    let wire_field = wire_field?;
                    if wire_field.field.field_number != tag.extension_number() {
                        continue;
                    }

                    matched_field = true;
                    value.parse_merge(&wire_field.field)?;
                }

                // NOTE: The assumption is that each field is one WireField.
                if matched_field {
                    self.unknown_fields.fields.remove(field_i);
                } else {
                    field_i += 1;
                }
            }

            extensions.insert(
                tag.extension_number(),
                Extension {
                    value,
                    name: tag.extension_name(),
                },
            );
        }

        let e = extensions.get_mut(&tag.extension_number()).unwrap();

        // Light protection against number collisions.
        if e.name != tag.extension_name() {
            return Err(WireError::BadDescriptor);
        }

        Ok(&mut e.value)
    }

    /*
    pub fn get<'a, T: StaticExtensionTag>(
        &'a self,
        tag: &T,
    ) -> WireResult<ExtensionRef<'a, T::ExtensionType>> {
        let v = self.get_dynamic(tag)?;
        Ok(tag
            .downcast_extension_value(v)
            .ok_or(WireError::BadDescriptor)?)
    }

    pub fn get_mut<'a, T: StaticExtensionTag>(
        &'a mut self,
        tag: &T,
    ) -> WireResult<&'a mut T::ExtensionType> {
        let v = self.get_dynamic_mut(tag)?;

        Ok(tag
            .downcast_extension_value_mut(v)
            .ok_or(WireError::BadDescriptor)?)
    }
    */

    // pub fn iter<'a>(&'a self) -> impl Iterator<Item = (&ExtensionNumberType,
    // &Extension)> + 'a {     self.extensions
    //         .iter()
    //         .unwrap_or(&ExtensionSet::DEFAULT.extensions)
    //         .iter()
    // }
}
