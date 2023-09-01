use alloc::vec::Vec;

use common::const_default::ConstDefault;
use common::errors::*;
use common::{bytes::Bytes, list::Appendable};

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
    pub fn serialize_to<A: Appendable<Item = u8> + ?Sized>(&self, out: &mut A) -> Result<()> {
        for field in &self.fields {
            out.extend_from_slice(&field[..]);
        }

        Ok(())
    }
}
