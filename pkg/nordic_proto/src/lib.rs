#![feature(
    lang_items,
    type_alias_impl_trait,
    inherent_associated_types,
    alloc_error_handler,
    generic_associated_types
)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;
extern crate crypto;
extern crate protobuf;

pub mod constants;
pub mod keyboard_packet;
pub mod packet;
pub mod packet_cipher;
pub mod proto;
pub mod request_type;

pub mod usb_descriptors {
    include!(concat!(env!("OUT_DIR"), "/src/usb_descriptors.rs"));
}
