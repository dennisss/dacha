#[macro_use]
extern crate macros;

use common::errors::*;
use file::LocalPathBuf;
use image::{format::jpeg::encoder::JPEGEncoder, Image};

#[derive(Args)]
struct Args {
    input_path: LocalPathBuf,
    output_path: LocalPathBuf,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let image = Image::<u8>::read(&args.input_path).await?;

    let encoded = JPEGEncoder::new(95);
    let mut out = vec![];
    encoded.encode(&image, &mut out);

    file::write(&args.output_path, &out).await?;

    Ok(())
}
