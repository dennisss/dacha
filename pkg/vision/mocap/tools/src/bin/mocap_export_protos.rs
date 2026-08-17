/*
Exports the .proto API definitions to the 'mocap-client' repo

rm -r ../mocap-client/protos
cargo run --bin mocap_export_protos -- --output_dir=../mocap-client/protos
*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::io::Read;
use std::sync::Arc;
use std::{fs::File, time::Duration};
use std::time::Instant;

use common::errors::*;
use common::io::{Readable, Writeable};
use file::{LocalPathBuf, LocalPath};
use file::{project_dir, project_path};
use mocap_proto::mocap::*;
use protobuf::Message;
use protobuf_dynamic::*;


#[derive(Args)]
struct Args {
    output_dir: LocalPathBuf
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let descriptor_pool = DescriptorPool::new(DescriptorPoolOptions::default_for_workspace(
        &project_dir(),
    ));

    let input_dir = project_path!("pkg/vision/mocap/proto");

    let mut input_paths = vec![];
    file::recursively_list_dir(&input_dir, &mut |path: &LocalPath| {
        if path.extension().unwrap_or_default() != "proto" {
            return;
        }

        input_paths.push(path.to_owned());
    });

    for path in input_paths {
        descriptor_pool.add_file(path).await?;
    }

    let all_files = descriptor_pool.all_files();

    for proto_file in all_files {
        let abs_path = proto_file.local_path().unwrap();
        let rel_path = abs_path.strip_prefix(&project_dir()).unwrap();

        println!("Syncing: {}", rel_path.display());

        let output_path = args.output_dir.join(rel_path);
        file::create_dir_all(output_path.parent().unwrap()).await?;
        file::copy(&abs_path, &output_path).await?;
    }

    Ok(())
}
