extern crate protobuf_compiler;

fn main() {
    let mut options = protobuf_compiler::CompilerOptions::default();
    options.runtime_package = "crate".into();

    protobuf_compiler::build_with_options(options).unwrap();
}
