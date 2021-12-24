#![feature(
    proc_macro_hygiene,
    decl_macro,
    generators,
    trait_alias,
    core_intrinsics,
    entry_insert
)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate common;
#[cfg(feature = "std")]
extern crate parsing; // < Mainly needed for f32/f64 conversions

#[macro_use]
extern crate macros;

#[cfg(feature = "std")]
extern crate json;
// #[cfg(feature = "std")]
// extern crate protobuf_compiler;
#[cfg(feature = "std")]
extern crate protobuf_descriptor;

#[cfg(feature = "std")]
mod descriptor_pool;
#[cfg(feature = "std")]
pub mod dynamic;
mod proto;

// TODO: Remove this 'use' statement.
#[cfg(feature = "std")]
pub use common::bytes::{Bytes, BytesMut};
#[cfg(feature = "std")]
pub use descriptor_pool::*;
#[cfg(feature = "std")]
pub use dynamic::*;
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
