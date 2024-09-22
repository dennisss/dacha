#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use common::errors::*;
use compression::transform::transform_to_vec;
use file::project_dir;

#[executor_main]
async fn main() -> Result<()> {
    let files = &[
        "testdata/gutenberg/shakespeare.txt",
        "testdata/random/random_100",
        "testdata/random/random_463",
        "testdata/random/random_4096",
        "testdata/random/random_1048576",
    ];

    for file in files {
        println!("===  {}", file);

        let path = project_dir().join(file);
        let data = file::read(&path).await?;

        let options = heatshrink::Options {
            window_bits: 11,
            lookahead_bits: 4,
        };

        let mut compressed = vec![];
        transform_to_vec(
            heatshrink::Encoder::new(options.clone())?,
            &data,
            &mut compressed,
        )?;

        let mut uncompressed = vec![];
        transform_to_vec(
            heatshrink::Decoder::new(options.clone())?,
            &compressed,
            &mut uncompressed,
        )?;

        assert_eq!(&uncompressed, &data);

        println!(
            "Ratio: {}",
            (compressed.len() as f32) / (uncompressed.len() as f32)
        );
    }

    Ok(())
}
