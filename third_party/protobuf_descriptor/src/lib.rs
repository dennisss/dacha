#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

extern crate protobuf_core;
#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

pub mod descriptor;
pub use descriptor::*;
