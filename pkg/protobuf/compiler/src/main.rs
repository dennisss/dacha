#[macro_use]
extern crate common;

use common::errors::*;
use file::project_path;

/*
Run with
    cargo run --package protobuf_compiler --no-default-features
followed by
    cargo run --package protobuf_compiler
*/

fn main() -> Result<()> {
    let dir = file::project_path!("third_party/protobuf_descriptor");
    let mut options = protobuf_compiler::project_default_options();
    options.runtime_package = "protobuf_core".into();
    options.should_format = true;

    protobuf_compiler::build_custom(&dir, &dir, options)?;

    let dir = file::project_path!("pkg/protobuf/compiler/proto");
    let mut options = protobuf_compiler::project_default_options();
    options.runtime_package = "protobuf_core".into();
    options.should_format = true;

    protobuf_compiler::build_custom(&dir, &dir, options)?;

    Ok(())
}
