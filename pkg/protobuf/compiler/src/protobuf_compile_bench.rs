/*
Benchmarks the performance of the protobuf compiler using the 'googleapis' repository which contains
100s of proto files.
*/

extern crate common;
extern crate protobuf_compiler;

use std::time::Instant;

use common::errors::*;
use file::project_path;
use file::temp::TempDir;
use protobuf_compiler::CompilerOptions;

fn main() -> Result<()> {
    let output_dir = TempDir::create()?;

    let mut options = protobuf_compiler::project_default_options();
    options.allowlisted_paths = Some(vec![
        project_path!("third_party/googleapis/repo/google/rpc"),
        project_path!("third_party/googleapis/repo/google/spanner"),
        project_path!("third_party/googleapis/repo/google/longrunning"),
        project_path!("third_party/googleapis/repo/google/iam"),
        project_path!("third_party/googleapis/repo/google/api"),
        project_path!("third_party/googleapis/repo/google/type"),
        project_path!("third_party/googleapis/repo/google/logging"),
    ]);

    let mut start = Instant::now();

    protobuf_compiler::build_custom(
        &project_path!("third_party/googleapis"),
        output_dir.path(),
        options,
    );

    let mut end = Instant::now();

    println!("Took {:?}", end - start);

    Ok(())
}
