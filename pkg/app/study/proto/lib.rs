#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

extern crate common;
extern crate protobuf;
#[macro_use]
extern crate macros;

include!(concat!(env!("OUT_DIR"), "/proto_lib.rs"));
