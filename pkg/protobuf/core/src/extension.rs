use alloc::boxed::Box;
use core::any::Any;
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

pub trait ExtensionTag {
    type ExtensionType: 'static;

    /// Used for wire field parsing/serialization.
    fn extension_number(&self) -> ExtensionNumberType;

    /// Fully qualified name of the extension field (of the form
    /// [package].[optional_sub_messages].[field])
    ///
    /// Used for text proto parsing/printing.
    fn extension_name(&self) -> StringPtr;

    /// Used for parsing value.
    fn default_extension_value(&self) -> Value;

    fn downcast_extension_value(&self, value: &Value) -> Option<&Self::ExtensionType>;
}

/// TODO: PartialEq of this must consider values in unknown_fields of the other
/// message.
#[derive(Default, Clone, PartialEq)]
pub struct ExtensionSet {
    extensions: Option<HashMap<ExtensionNumberType, Extension, SumHasherBuilder>>,
}

impl ConstDefault for ExtensionSet {
    const DEFAULT: Self = Self { extensions: None };
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
    pub fn serialize_to<A: Appendable<Item = u8> + ?Sized>(&self, out: &mut A) -> Result<()> {
        if let Some(extensions) = &self.extensions {
            for (num, ext) in extensions {
                ext.value.serialize_to(*num, out)?;
            }
        }

        Ok(())
    }

    // pub fn iter<'a>(&'a self) -> impl Iterator<Item = (&ExtensionNumberType,
    // &Extension)> + 'a {     self.extensions
    //         .iter()
    //         .unwrap_or(&ExtensionSet::DEFAULT.extensions)
    //         .iter()
    // }

    // Allow iterating over extensions

    // TODO: Should this return a default value if the extension is not present?
    pub fn extension<'a, T: ExtensionTag>(
        &'a mut self,
        tag: &'a T,
        unknown_fields: &mut UnknownFieldSet,
    ) -> WireResult<Option<&'a T::ExtensionType>> {
        let extensions = self
            .extensions
            .get_or_insert_with(|| HashMap::with_hasher(SumHasherBuilder::default()));

        if !extensions.contains_key(&tag.extension_number()) {
            // TODO: Eventually we should register all extension types at
            // compile time and parse them during regular protobuf message
            // parsing (as that's what the regular protobuf compiler does).

            let mut value = tag.default_extension_value();

            let mut found_fields = false;

            let mut field_i = 0;
            while field_i < unknown_fields.fields.len() {
                for wire_field in WireFieldIter::new(&unknown_fields.fields[field_i]) {
                    let wire_field = wire_field?;
                    if wire_field.field.field_number != tag.extension_number() {
                        continue;
                    }

                    found_fields = true;
                    value.parse_merge(&wire_field.field)?;
                }

                // NOTE: The assumption is that each field is one WireField.
                if found_fields {
                    unknown_fields.fields.remove(field_i);
                } else {
                    field_i += 1;
                }
            }

            if !found_fields {
                return Ok(None);
            }

            extensions.insert(
                tag.extension_number(),
                Extension {
                    value,
                    name: tag.extension_name(),
                },
            );
        }

        Ok(Some(
            tag.downcast_extension_value(&extensions.get(&tag.extension_number()).unwrap().value)
                .ok_or(WireError::BadDescriptor)?,
        ))
    }
}
