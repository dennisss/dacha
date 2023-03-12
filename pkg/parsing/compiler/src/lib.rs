extern crate alloc;
extern crate core;

extern crate protobuf;

#[macro_use]
extern crate macros;

mod buffer;
mod build;
mod compiler;
mod enum_type;
mod language;
mod primitive;
mod proto;
mod size;
mod struct_type;
mod types;

pub use build::build;
