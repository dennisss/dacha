#![feature(
    proc_macro_hygiene,
    decl_macro,
    generators,
    trait_alias,
    core_intrinsics
)]

#[macro_use]
extern crate common;
extern crate byteorder;
extern crate parsing; // < Mainly needed for f32/f64 conversions

// TODO: Eventually remove dependencies on the compiler
extern crate protobuf_compiler;

pub mod reflection;
pub mod service;
pub mod text;
pub mod wire;
mod proto;

pub use common::bytes::{Bytes, BytesMut};
use common::errors::*;
use crate::reflection::MessageReflection;
pub use protobuf_compiler::spec::EnumValue;
pub use protobuf_compiler::spec::FieldNumber;

// NOTE: Construct an empty proto by calling MessageType::default()
// Clone + std::fmt::Debug + std::default::Default + MessageReflection
pub trait Message {
    // NOTE: This will append values to
    fn parse(data: Bytes) -> Result<Self> where Self: Sized;

    /// Serializes the protobuf as a
    fn serialize(&self) -> Result<Vec<u8>>;

    // TODO: should be a shared reference?
    // fn descriptor() -> Descriptor;

    //	fn parse_text(data: &str) -> Result<Self>;

    // TODO: Must also be able to parse a text proto

    // TODO: Serializers must return a result because required conditions may
    // not be satisfied.

    //	fn debug_string(&self) -> String;

    //	fn merge_from(&mut self, other: &Self);

    // fn unknown_fields() -> &[UnknownField];
}

/// Common trait implemented by all code generated protobuf enum types.
pub trait Enum {
    /// Should convert a number to a valid branch of the enum, or else should
    /// error out it the value is not in the enum.
    fn parse(v: EnumValue) -> Result<Self>
    where
        Self: Sized;

    fn parse_name(name: &str) -> Result<Self>
    where
        Self: Sized;

    fn name(&self) -> &'static str;
    fn value(&self) -> EnumValue;

    fn assign(&mut self, v: EnumValue) -> Result<()>;
    // TODO: This is inconsistent with the other Message trait.

    fn assign_name(&mut self, name: &str) -> Result<()>;
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::proto::test::*;

    #[test]
    fn generated_code_usage() {
        let mut list = ShoppingList::default();

        assert_eq!(list.id(), 0);
        assert_eq!(list.items_len(), 0);
        assert_eq!(list.store(), ShoppingList_Store::UNKNOWN);

        // A protobuf with all default fields should have no custom fields.
        assert_eq!(&list.serialize().unwrap(), &[]);

        list.set_id(0);
        list.set_name("".to_string());
        assert_eq!(&list.serialize().unwrap(), &[]);

        list.set_id(4);
        assert_eq!(&list.serialize().unwrap(), &[0x10, 4]);



    }

}
