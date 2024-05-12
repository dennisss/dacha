#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use base_error::*;
use file::LocalPathBuf;

/*
cargo run --release --bin gcode_tile -- \
    --input=doc/pi_rack/poe-bridge/gcode/poe-bridge-F_Cu.gbr_iso_combined_cnc.nc \
    --output=doc/pi_rack/poe-bridge/gcode_tiled/poe_bridge_copper.nc

cargo run --release --bin gcode_tile -- \
    --input=doc/pi_rack/poe-bridge/gcode/poe-bridge.drl_cnc.nc \
    --output=doc/pi_rack/poe-bridge/gcode_tiled/poe_bridge_drill.nc

cargo run --release --bin gcode_tile -- \
    --input=doc/pi_rack/poe-bridge/gcode/poe-bridge-Edge_Cuts.gbr_cutout_cnc.nc \
    --output=doc/pi_rack/poe-bridge/gcode_tiled/poe_bridge_edge_cut.nc



*/

#[derive(Args)]
struct Args {
    input: LocalPathBuf,
    output: LocalPathBuf,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = base_args::parse_args::<Args>()?;

    let data = file::read(&args.input).await?;

    let out = gcode::tile_gcode(&data, (22.0, 22.0), (6, 1))?;

    file::write(&args.output, out).await?;

    // println!("{}", std::str::from_utf8(&out)?);

    Ok(())
}
