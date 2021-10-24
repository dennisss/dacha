#[macro_use]
extern crate common;

use common::errors::*;

fn main() -> Result<()> {
    let dir = project_path!("third_party/protobuf_descriptor");
    let mut options = protobuf_compiler::CompilerOptions::default();
    options.runtime_package = "protobuf_core".into();

    protobuf_compiler::build_custom(&dir, &dir, "protobuf_descriptor", options)
}
