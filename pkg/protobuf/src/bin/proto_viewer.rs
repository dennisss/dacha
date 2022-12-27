extern crate common;
extern crate protobuf;
#[macro_use]
extern crate macros;

use common::errors::*;
use protobuf::Message;

/*
Example of running:

cargo run --bin proto_viewer -- perf.pb --proto_file=third_party/google/src/proto/profile.proto --proto_type=perftools.profiles.Profile
*/

#[derive(Args)]
struct Args {
    #[arg(positional)]
    path: String,

    proto_file: Option<String>,

    proto_type: Option<String>,
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let data = file::read(file::current_dir()?.join(&args.path)).await?;

    let mut descriptor_pool = protobuf::DescriptorPool::new();

    // TODO: Deduplicate some of this logic with the compiler.
    if let Some(path) = args.proto_file {
        // TODO: Implement a new Path struct which enforces that join never follows
        // directory traversals.
        let path = file::current_dir()?.join(&path);

        descriptor_pool.add_local_file(path).await?;

        // TODO: Convert to result.
        let type_name = args.proto_type.unwrap();

        let type_desc = descriptor_pool
            .find_relative_type("", &type_name)
            .ok_or_else(|| format_err!("Unknown type named: {}", type_name))?
            .to_message()
            .ok_or_else(|| format_err!("Type isn't a message: {}", type_name))?;

        let mut msg = protobuf::DynamicMessage::new(type_desc);
        msg.parse_merge(&data)?;

        println!(
            "Text Format: {}",
            protobuf::text::serialize_text_proto(&msg)
        );

        return Ok(());
    }

    protobuf::viewer::print_message(&data, "")?;

    Ok(())
}

fn main() -> Result<()> {
    executor::run(run())?
}
