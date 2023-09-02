extern crate common;
#[macro_use]
extern crate parsing;
extern crate protobuf_core;
#[cfg(feature = "descriptors")]
extern crate protobuf_descriptor;
#[macro_use]
extern crate regexp_macros;

mod build;
mod compiler;
mod escape;

pub use build::{build, build_custom, build_with_options, project_default_options};
pub use compiler::CompilerOptions;
