#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::time::Instant;

use base_error::*;
use cam::{kicad::KicadPCBExport, process_pcb};
use file::{temp::TempDir, LocalPathBuf};
use gerber::{
    excellon,
    graphics::{FillMode, GraphicsObject},
    processor::{CommandsProcessor, CommandsProcessorOptions},
};
use graphics::{
    canvas::{Paint, Path, PathBuilder},
    opengl::{canvas::OpenGLCanvas, canvas_render_loop::CanvasFrameHandler},
    raster::canvas_render_loop::WindowOptions,
};
use math::geometry::{
    bounding_box::BoundingBoxBuilder,
    half_edge::{FaceDebug, HalfEdgeStruct},
};

/*
Endoscope mount uses 2 x M2 16mm screws
- Keywords to search for are 'Windows' or 'Linux' or 'UVC' or presence of a male USB type A connector (the 3 in 1 endoscopes)


Dual layer workflow:
- Given full workpiece size
- Secure full PCB to the area with weak double sided tape.
- Probe the surface of the PCB
- Drill 4 asymetric alignment holes in the corners
- Verify position of the holes with the camera
- Do the isolation routing for the top side
- Do the solder mask for the top side
- Flip the PCB over
- Check the location of the holes
- Do Z probe
- Transform gcode appropriately
- Do back isolation routing
- Do back solder mask
- Do drilling
- Do edge cuts.




TODO: Need to investigate what the solder mask removal quality is so bad.

TODO: Make this a golden test case:

    cargo run --bin cam --release -- \
        --board_path=pkg/cnc/boards/usb_power_switch/usb_power_switch.kicad_pcb \
        --output_path=usb_power_switch.gcode


    cargo run --bin cam --release -- \
        --board_path=pkg/things/fan_controller/boards/board-hl15-latest/board-hl15-latest.kicad_pcb \
        --forced_hole_diameter=0.9 \
        --output_path=fan_controller.gcode


TODO: For Carvera leveling, if in 'preview' mode, then the view box will move while probing
- Also need a clear sense of the progress o leveling

TODO: Carvera layer previews are broken

TODO: Better carvera vacuum (ideally one with more part visibility)
- For the existing one I ened to do a better job of ensuring that the vacuum tue isn't preventing it from going all the way down.

TODO: Get a replacement Carvera spindle cover

TODO: Need an estimate for how long a whole job will take.
- Challenging part is to estimate the intermediate steps.

TODO: Need an alarm for the UV curing time (ideally make this computer controlled) and when the job is paused, we need user messages.

TODO: Need more tiling simplification:
- Don't need to turn off and on spindle in between the tile runs.

TODO: Solder mask not getting completely removed
- Need to go slower or more overlap?
- It's mostly on the inner most parts which probably have very short cut times
=> Partly fixing with more overlap.

- TODO: chips getting stuck high up on the corn bit

TODO: Still seeing the random flakiness in serial

TODO: Wireless probing auto-suggest # of points based on x/y distance

*/

#[derive(Args)]
struct Args {
    board_path: LocalPathBuf,
    output_path: LocalPathBuf,

    #[arg(default = 0.0)]
    forced_hole_diameter: f32,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut config = cam_proto::cnc::PCBProcessorConfig::default();
    protobuf::text::parse_text_proto(
        "
        isolation {
            tool_index: 2
            tool_diameter: 0.2
            tool_v_angle: 30
            min_cut_depth: 0.05 # TODO
            # cut_width: 0.23 # For ~0.05mm cut depth.
            cut_depth: 0.05
            num_passes: 4
            overlap_percentage: 0.1
            spindle_speed: 12000
            travel_z: 1
            clearance_z: 10
            feedrate_xy: 500
            feedrate_z: 200
            rapid_feedrate: 1000
        }

        # TODO: Will this well cover the center patch of pads.
        # TODO: Need min time at start and end point of each path?
        mask_removal {
            tool_index: 5
            tool_diameter: 0.3
            spindle_speed: 6000
            # Standard is 0.2 which is not enough force for well cured mask.
            cut_depth: 0.3
            overlap_percentage: 0.3
            travel_z: 1
            clearance_z: 10
            feedrate_z: 200
            feedrate_xy: 400
            rapid_feedrate: 1000
            inverted: true
            erosion: 0.05
            multiples: 2
        }

        paste_stencil {
            tool_index: 2
            tool_diameter: 0.2
            tool_v_angle: 30
            spindle_speed: 12000
            feedrate_z: 200
            feedrate_xy: 100
            rapid_feedrate: 1000
            cut_depth: 0.3
            travel_z: 1
            clearance_z: 10
            num_passes: 1
            inverted: true
        }

        drill {
            tool_index: 3
            # tool_diameter: 0.8
            spindle_speed: 12000
            rapid_feedrate: 1000
            feedrate_z: 200
            travel_z: 1
            clearance_z: 10
            drill_z: -1.62
        }
    
        cutout {
            tool_index: 3
            tool_diameter: 0.8
            margin: 0.02
            feedrate_xy: 400
            feedrate_z: 300
            travel_z: 1
            clearance_z: 10
            cut_depth_z: 1.65
            depth_per_pass_z: 0.2
            spindle_speed: 12000
            rapid_feedrate: 1000
        }
        ",
        &mut config,
    )?;

    config.set_forced_hole_diameter(args.forced_hole_diameter);

    // TODO: Verify all feedrates are non-zero. Verify all travel/clearance z
    // heights are non-zero

    let tmp_dir = TempDir::create()?;
    let export = KicadPCBExport::generate(&args.board_path, tmp_dir.path())?;

    let options = cam::PCBProcessorOptions {
        config,
        edge_cuts_path: Some(export.edge_cuts),
        back_copper_path: Some(export.back_copper),
        back_mask_path: Some(export.back_mask),
        back_paste_path: Some(export.back_paste),
        drill_path: Some(export.drill_file),
        min_feature_size: 0.02,
    };

    // TODO: Need to verify that we well handle when contours get very small (e.g.
    // attempting to offline a circle/obround with close to its diameter may
    // collapse to a point or a line).

    // TODO: Warn when there is a solder mask hole for every pad labeled in the
    // copper layer.

    let start = Instant::now();
    let program = process_pcb(&options).await?;
    let end = Instant::now();

    println!("Processing Time: {:?}", end - start);

    file::write(&args.output_path, &program).await?;

    Ok(())
}
