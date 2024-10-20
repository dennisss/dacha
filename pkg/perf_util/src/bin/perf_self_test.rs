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
    let task = executor::spawn(perf::busy::task1());
    let task2 = executor::spawn(perf::busy::task2());

    let profile = perf::profile_self(Duration::from_secs(5)).await?;

    task.cancel().await;
    task2.cancel().await;

    let mut data = profile.serialize()?;
    file::write("perf.pb", &data).await?;

    let mut data_gz = vec![];

    compression::transform::transform_to_vec(
        GzipEncoder::default_without_metadata(),
        &data,
        &mut data_gz,
    )?;

    println!("Write : {}", data_gz.len());

    file::write("perf.pb.gz", &data_gz).await?;

    Ok(())
}
