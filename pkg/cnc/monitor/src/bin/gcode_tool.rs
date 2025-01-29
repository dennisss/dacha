/*

cargo run --bin gcode_tool --release -- \
    usb_power_switch.gcode \
    --preset=makera_carvera \
    --output_dir=layers


cargo run --bin gcode_tool --release -- \
    $PWD/testdata/cnc/3DBenchy_0.4n_0.2mm_PETG_XL_59m.bgcode \
    --preset=prusa_xl \
    --output_dir=$PWD/layers

$PWD/testdata/cnc/3DBenchy_0.2mm_PETG_MK3S_1h23m.gcode \
--preset=prusa_i3_mk3sp

$PWD/testdata/cnc/3DBenchy_0.4n_0.2mm_PETG_XLIS_48m.gcode \
--preset=prusa_xl

/media/dennis/carvera-inner/gcodes/Examples/LED/PCB-UV-MASK(PART1).nc \
--preset=makera_carvera

$PWD/testdata/cnc/xyzCalibration_cube_0.4n_0.2mm_PLA_XL_50m.gcode \
--preset=prusa_xl

$PWD/testdata/cnc/sata-top_0.2mm_PETG_MK3S_28m.gcode
--preset=prusa_i3_mk3sp

*/

#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::time::Instant;

use base_error::*;
use cnc_monitor::program::new_progress_tracker;
use common::io::Writeable;
use file::LocalPathBuf;

#[derive(Args)]
struct Args {
    #[arg(positional)]
    path: LocalPathBuf,

    preset: String,

    output_dir: LocalPathBuf,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let preset = cnc_monitor::presets::get_machine_presets()
        .await?
        .into_iter()
        .find(|p| p.base_config() == args.preset)
        .ok_or_else(|| format_err!("Unknown preset: {}", args.preset))?;

    file::create_dir_all(&args.output_dir).await?;

    let start = Instant::now();

    let (s, r) = new_progress_tracker();

    let summary = cnc_monitor::program::ProgramSummary::create(&args.path, 1000, s).await?;

    let end = Instant::now();

    println!("Summary Compute Time: {:?}", end - start);

    println!("{:#?}", summary.proto);

    if let Some(thumb) = summary.best_thumbnail()? {
        file::write(args.output_dir.join("thumbnail.jpg"), thumb).await?;
    }

    // let profile = executor::spawn(perf::profile_self(Duration::from_secs(10)));

    let start = Instant::now();

    let vis = cnc_monitor::program_preview::ProgramPreview::create(
        &args.path,
        &preset,
        &summary.proto,
        None,
        true,
    )
    .await?;

    let end = Instant::now();

    println!("Preview Compute Time: {:?}", end - start);

    // let profile = profile.join().await?;
    // file::write(project_path!("perf.pb"), profile.serialize()?).await?;

    println!("Vis:");
    println!("{:?}", vis.proto);

    {
        let mut f = file::LocalFile::open_with_options(
            args.output_dir.join("layers.bin.zz"),
            file::LocalFileOpenOptions::new().write(true).create(true),
        )?;

        for part in vis.layers_image {
            f.write_all(&part).await?;
        }

        f.flush().await?;
    }

    for (i, data) in vis.layer_jpegs.iter().enumerate() {
        file::write(args.output_dir.join(format!("{:08}.jpg", i)), data).await?;
    }

    Ok(())
}
