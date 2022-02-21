#![feature(
    lang_items,
    asm,
    type_alias_impl_trait,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types
)]
#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;
extern crate protobuf;

pub mod constants;
pub mod packet;
pub mod proto;
pub mod usb;
