extern crate parsing_compiler;
extern crate protobuf_compiler;

fn main() {
    parsing_compiler::build().unwrap();
    protobuf_compiler::build().unwrap();
}
