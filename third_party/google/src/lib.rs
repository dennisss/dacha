#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

extern crate common;
extern crate protobuf_core;
#[macro_use]
extern crate macros;

pub mod proto;
