#[macro_use]
extern crate common;
extern crate protobuf;
#[macro_use]
extern crate macros;
extern crate rpc;

pub mod reflection {
    include!(concat!(env!("OUT_DIR"), "/src/reflection.rs"));
}
