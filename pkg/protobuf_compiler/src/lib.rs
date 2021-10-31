extern crate common;
#[macro_use]
extern crate parsing;
extern crate protobuf_core;
extern crate protobuf_descriptor;

mod build;
mod compiler;
pub mod spec;
pub mod syntax;

pub use build::{build, build_custom, build_with_options};
pub use compiler::CompilerOptions;
