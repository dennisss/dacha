extern crate common;
extern crate compression;
extern crate perf;
extern crate protobuf;
#[macro_use]
extern crate macros;

use std::time::Duration;

use common::errors::*;
use compression::gzip::*;
use protobuf::Message;

#[executor_main]
async fn main() -> Result<()> {
    let profile = perf::profile_self(Duration::from_secs(5)).await?;
    println!("Profile: {:?}", profile);

    let mut data = profile.serialize()?;
    file::write("perf_custom.pb", &data).await?;

    let mut data_gz = vec![];

    compression::transform::transform_to_vec(
        GzipEncoder::default_without_metadata(),
        &data,
        &mut data_gz,
    )?;

    println!("Write : {}", data_gz.len());

    file::write("perf_custom.pb.gz", &data_gz).await?;

    Ok(())
}
