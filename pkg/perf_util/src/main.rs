extern crate common;
extern crate compression;
extern crate perf;
extern crate protobuf;

use std::time::Duration;

use common::async_std::fs;
use common::errors::*;
use compression::gzip::*;
use protobuf::Message;

async fn run() -> Result<()> {
    let profile = perf::profile_self(Duration::from_secs(5)).await?;
    println!("Profile: {:?}", profile);

    let mut data = profile.serialize()?;
    fs::write("perf_custom.pb", &data).await?;

    let mut data_gz = vec![];

    let mut encoder = GzipEncoder::default_without_metadata();
    compression::transform::transform_to_vec(&mut encoder, &data, true, &mut data_gz)?;

    println!("Write : {}", data_gz.len());

    fs::write("perf_custom.pb.gz", &data_gz).await?;

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())
}
