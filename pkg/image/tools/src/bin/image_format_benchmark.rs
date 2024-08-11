// Benchmarks the performance of image encoding/decoding.

#[macro_use]
extern crate macros;

use std::time::{Duration, Instant};

use base_error::*;
use file::{project_path, LocalPathBuf};
use image::{format::jpeg::encoder::JPEGEncoder, Image};
use protobuf::Message;

#[executor_main]
async fn main() -> Result<()> {
    let image = Image::<u8>::zero(1920, 1080, image::Colorspace::RGB);

    let image = Image::<u8>::read("testdata/image/nyhavn-1920x1080.jpg").await?;

    let encoder = JPEGEncoder::new(95);
    let mut n = 0;

    let profile = executor::spawn(perf::profile_self(Duration::from_secs(5)));

    const ITERS: usize = 20;

    let start = Instant::now();
    for i in 0..ITERS {
        let mut out = vec![];
        encoder.encode(&image, &mut out)?;
        n += out.len();
    }
    let end = Instant::now();

    assert!(n > 0);

    println!("{:?}", (end - start).as_secs_f64() / (ITERS as f64));

    let profile = profile.join().await?;
    file::write(project_path!("perf.pb"), profile.serialize()?).await?;

    Ok(())
}
