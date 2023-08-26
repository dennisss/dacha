extern crate alloc;
extern crate core;

extern crate protobuf;

extern crate parsing_compiler_proto;

#[macro_use]
extern crate macros;

mod buffer;
mod build;
mod compiler;
mod enum_type;
mod expression;
mod primitive;
mod string;
mod struct_type;
mod types;
mod union_type;

pub use build::build;

use parsing_compiler_proto as proto;
