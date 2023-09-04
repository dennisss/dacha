extern crate file;
extern crate protobuf_compiler;

use file::project_path;

fn main() {
    let mut options = protobuf_compiler::project_default_options();
    options.runtime_package = "protobuf_core".into();

    protobuf_compiler::build_with_options(options).unwrap();
}
