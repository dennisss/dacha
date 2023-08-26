extern crate protobuf_compiler;

#[macro_use]
extern crate file;

fn main() {
    let mut options = protobuf_compiler::project_default_options();
    options.allowlisted_paths = Some(vec![project_path!(
        "third_party/googleapis/repo/google/rpc"
    )]);

    protobuf_compiler::build_with_options(options).unwrap();
}
