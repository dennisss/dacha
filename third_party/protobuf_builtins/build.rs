extern crate file;
extern crate protobuf_compiler;

use file::project_path;

fn main() {
    let mut options = protobuf_compiler::CompilerOptions::default();
    options.runtime_package = "protobuf_core".into();
    options
        .paths
        .push(project_path!("third_party/protobuf_builtins/proto"));

    protobuf_compiler::build_with_options(options).unwrap();
}
