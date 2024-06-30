#![feature(
    proc_macro_hygiene,
    decl_macro,
    trait_alias,
    core_intrinsics,
    concat_idents
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
extern crate protobuf_builtins;
#[cfg(feature = "std")]
extern crate protobuf_descriptor;

#[cfg(feature = "std")]
pub mod viewer;

// TODO: Remove this 'use' statement.
#[cfg(feature = "std")]
pub use common::bytes::{Bytes, BytesMut};
pub use protobuf_core::*;
#[cfg(feature = "std")]
pub use protobuf_dynamic::*;
