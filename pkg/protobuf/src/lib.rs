#![feature(
    proc_macro_hygiene,
    decl_macro,
    generators,
    trait_alias,
    core_intrinsics,
    entry_insert
)]

#[macro_use]
extern crate common;
extern crate parsing; // < Mainly needed for f32/f64 conversions

#[macro_use]
extern crate macros;

extern crate json;
extern crate protobuf_compiler;
extern crate protobuf_descriptor;

mod descriptor_pool;
pub mod dynamic;
mod proto;

pub use common::bytes::{Bytes, BytesMut};
pub use descriptor_pool::*;
pub use dynamic::*;
pub use protobuf_core::EnumValue;
pub use protobuf_core::FieldNumber;
pub use protobuf_core::*;

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
