#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::time::Instant;
use std::f32::consts::PI;
use std::collections::HashMap;
use std::collections::HashSet;

use base_error::*;
use common::line_builder::LineBuilder;
use cam::*;
use file::{temp::TempDir, LocalPathBuf};
use gerber::{
    excellon,
    graphics::{FillMode, GraphicsObject},
    processor::{CommandsProcessor, CommandsProcessorOptions},
};
use cam_proto::cnc::CutOutProcessorConfig;
use graphics::{
    canvas::{Paint, Path, PathBuilder},
    opengl::{canvas::OpenGLCanvas, canvas_render_loop::CanvasFrameHandler},
    raster::canvas_render_loop::WindowOptions,
};
use math::matrix::vec2f;
use cam_proto::cnc::ArcMotionBuilderConfig;
use cam::cutout::CutOutProcessorOptions;
use cam::cutout::CutOutProcessor;
use cam::edge::EdgeCutMetadata;


#[derive(Args)]
struct Args {
    output_path: LocalPathBuf,
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut config = CutOutProcessorConfig::default();
    protobuf::text::parse_text_proto(r#"
        tool_index: 4
        tool_diameter: 3.175
        margin: 0.02
        feedrate_xy: 400
        feedrate_z: 200
        travel_z: 1
        clearance_z: 10
        cut_depth_z: 1.6
        depth_per_pass_z: 0.4
        spindle_speed: 10000
        rapid_feedrate: 1000
    "#, &mut config)?;


    let mut holes = vec![
        gerber::DrillHole { x: 7.5, y: 85.5, diameter: 5.4 },
        gerber::DrillHole { x: 7.5, y: 27.5, diameter: 5.4 },
        gerber::DrillHole { x: 56.5, y: 85.5, diameter: 5.4 },
        gerber::DrillHole { x: 56.5, y: 27.5, diameter: 5.4 },
    ];

    let mut edge_cuts = vec![];

    for mut hole in holes {
        let mut path_builder = PathBuilder::new();
        path_builder.ellipse(
            vec2f(hole.x, hole.y),
            vec2f(hole.diameter, hole.diameter) / 2.0,
            0.0,
            2.0 * PI,
        );
        path_builder.close();

        let obj = gerber::GraphicsObject {
            paths: vec![gerber::GraphicsPath {
                path: path_builder.build(),
                fill: gerber::FillMode::Dark,
            }],
            line: None,
            attributes: HashMap::new(),
        };

        edge_cuts.push(obj);


    }

    let mut edge_meta = EdgeCutMetadata {
        outer_edge_path: vec![],
        outer_edge_objects: HashSet::new(),
        inner_edge_objects: HashSet::new()
    };

    for i in 0..edge_cuts.len() {
        edge_meta.inner_edge_objects.insert(i);
    }

    let mut program = LineBuilder::new();
    program.add("G21 G40 G54");
    program.add("G80 G90 G94");

    // TODO: The initial 'G00 Z10 F1000' will mess up the cnc_monitor min_position estimates and think it is x_0/y_0.



    let mut arc_config = ArcMotionBuilderConfig::default();
    arc_config.set_min_points(3u32);


    let cutout_processor = CutOutProcessor::new(CutOutProcessorOptions {
        config: config,
        max_error: 0.01,
        arc_config,
    });

    cutout_processor.process(&edge_cuts, &edge_meta, &mut program)?;

    file::write(&args.output_path, program.to_string().as_bytes()).await?;

    Ok(())
}