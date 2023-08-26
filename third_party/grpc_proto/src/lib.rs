#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate common;
extern crate protobuf;
#[macro_use]
extern crate macros;

include!(concat!(env!("OUT_DIR"), "/proto_lib.rs"));

// TODO: Change the files in this directory to make sure that 'grpc.proto' gets
// the correct file descriptor name.
