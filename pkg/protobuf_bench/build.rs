extern crate protobuf_compiler;

fn main() {
    let mut options = protobuf_compiler::CompilerOptions::default();
    protobuf_compiler::build_with_options(options).unwrap();
}
