#[macro_use]
extern crate common;

use common::errors::*;

/*
Run with
    cargo run --package protobuf_compiler --no-default-features
followed by
    cargo run --package protobuf_compiler
*/

fn main() -> Result<()> {
    let dir = file::project_path!("third_party/protobuf_descriptor");
    let mut options = protobuf_compiler::CompilerOptions::default();
    options.runtime_package = "protobuf_core".into();
    // options.paths.push("dir");
    options.should_format = true;

    protobuf_compiler::build_custom(&dir, &dir, options)
}
