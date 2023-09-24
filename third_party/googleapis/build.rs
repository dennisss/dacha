extern crate protobuf_compiler;

#[macro_use]
extern crate file;

fn main() {
    let mut options = protobuf_compiler::project_default_options();
    options.allowlisted_paths = Some(vec![
        project_path!("third_party/googleapis/repo/google/rpc"),
        project_path!("third_party/googleapis/repo/google/spanner"),
        project_path!("third_party/googleapis/repo/google/longrunning"),
        project_path!("third_party/googleapis/repo/google/iam"),
        project_path!("third_party/googleapis/repo/google/api"),
        project_path!("third_party/googleapis/repo/google/type"),
        project_path!("third_party/googleapis/repo/google/logging"),
        project_path!("third_party/googleapis/repo/google/storage"),
        project_path!("third_party/googleapis/repo/google/cloud/compute"),
        project_path!("third_party/googleapis/repo/google/cloud/extended_operations.proto"),
        project_path!("third_party/googleapis/rest"),
    ]);

    protobuf_compiler::build_with_options(options).unwrap();
}
