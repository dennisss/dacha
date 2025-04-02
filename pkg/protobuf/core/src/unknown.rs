use alloc::vec::Vec;

use common::const_default::ConstDefault;
use common::errors::*;
use common::{bytes::Bytes, list::Appendable};

use crate::message::{OutputBuffer, SerializeOptions};
use crate::wire::{WireResult, WireError};

/// Set of unknown fields/extensions which were're referenced in the main schema
/// of a message.
///
/// NOTE: Unlike the regular protobuf implementation, they may also include
/// extensions which were compiled into the binary but weren't read by a user
/// yet.
///
/// TODO: PartialEq is not well defined here.
#[derive(Default, Clone, PartialEq)]
pub struct UnknownFieldSet {
    /// Unparsed fields left over when parsing a binary proto.
    /// Each of these is corresponds to one WireField.
    ///
    /// TODO: Make this private?
    pub fields: Vec<Bytes>,
}

impl ConstDefault for UnknownFieldSet {
    const DEFAULT: Self = Self { fields: Vec::new() };
}

impl UnknownFieldSet {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn serialize_to(&self, options: &SerializeOptions, out: &mut OutputBuffer) -> WireResult<()> {
        if options.deterministic && !self.is_empty() {
            return Err(WireError::UnknownFieldsDropped);
        }

        for field in &self.fields {
            out.extend_from_slice(&field[..]);
        }

        Ok(())
    }
}
