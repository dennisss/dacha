#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::time::{Duration, Instant};

use base_error::*;
use crypto::hasher::Hasher;
use parsing::parse_next;

#[executor_main]
async fn main() -> Result<()> {
    //
    // "testdata/cnc/3DBenchy_0.2mm_PETG_MK3S_1h23m.gcode" // 86ms
    // "testdata/cnc/TapeCartridge_0.4n_0.25mm_PETG_XLIS_4h3m.bgcode" // 550ms
    let data = file::read(project_path!(
        "testdata/cnc/GalaxyCoaster-4x_0.4n_0.2mm_PLA,PLA,PLA,PLA,PLA_XL_8h32m.bgcode"
    ))
    .await?;

    // let profile = executor::spawn(perf::profile_self(Duration::from_secs(5)));

    let start = Instant::now();

    /*
    {
        let mut input = &data[..];

        let mut decoder = bgcode::RawDecoder::new();

        let mut out = vec![0u8; 4 * 1024];
        loop {
            let progress = decoder.update(input, true, &mut out)?;
            input = &input[progress.input_read..];

            // println!("{:?} : {}", progress.event, progress.output_written);

            if progress.done {
                break;
            }
        }
    }
    */

    /*
    {
        let mut input = &data[..];

        let mut decoder = bgcode::Decoder::new();

        loop {
            let mut out = vec![0u8; 128 * 1024];

            let progress = decoder.update(input, true, &mut out)?;
            input = &input[progress.input_read..];

            println!("{:?} : {}", progress.event, progress.output_written);
            if progress.output_written == 1 {
                println!("{:?}", out[0]);
            }

            if progress.done {
                break;
            }
        }
    }
    */

    for _ in 0..5 {
        let mut input = &data[..];

        let mut parser = bgcode::ProgramParser::default();

        let mut elements = vec![];
        while !input.is_empty() {
            let n = parser.parse_line(input, true, &mut elements)?;
            input = &input[n..];

            // println!("{:?}", elements);
            elements.clear();
        }
    }

    let end = Instant::now();

    println!("{:?}", end - start);

    // let profile = executor::spawn(perf::profile_self(Duration::from_secs(5)));

    // let profile = profile.join().await?;
    // file::write(project_path!("perf.pb"), profile.serialize()?).await?;

    Ok(())
}
