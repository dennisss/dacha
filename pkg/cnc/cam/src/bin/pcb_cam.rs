#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::time::Instant;

use base_error::*;
use cam::*;
use kicad::export::KicadPCBExport;
use file::{temp::TempDir, LocalPathBuf};
use gerber::{
    excellon,
    graphics::{FillMode, GraphicsObject},
    processor::{CommandsProcessor, CommandsProcessorOptions},
};
use cam_proto::cnc::SideAlignmentData;
use graphics::{
    canvas::{Paint, Path, PathBuilder},
    opengl::{canvas::OpenGLCanvas, canvas_render_loop::CanvasFrameHandler},
    raster::canvas_render_loop::WindowOptions,
};
use math::geometry::{
    bounding_box::BoundingBoxBuilder,
    half_edge::{FaceDebug, HalfEdgeStruct},
};

#[derive(Args)]
struct Args {
    config_path: LocalPathBuf,

    board_path: LocalPathBuf,
    output_path: LocalPathBuf,

    mode: Mode,

    #[arg(default = 0.0)]
    forced_hole_diameter: f32,
}

#[derive(Args)]
pub enum Mode {
    #[arg(name = "single-front")]
    SingleSidedFront,
    #[arg(name = "single-back")]
    SingleSidedBack,
    #[arg(name = "double-front")]
    DoubleSidedFront,
    #[arg(name = "double-back")]
    DoubleSidedBack {
        alignment_data: LocalPathBuf
    },

    #[arg(name = "laser-stencil-front")]
    LaserStencilFront,
    #[arg(name = "laser-stencil-back")]
    LaserStencilBack
    
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let mut config = cam_proto::cnc::PCBProcessorConfig::default();

    let config_txtpb = file::read_to_string(&args.config_path).await?;
    protobuf::text::parse_text_proto(&config_txtpb, &mut config)?;

    config.set_forced_hole_diameter(args.forced_hole_diameter);

    // TODO: Verify all feedrates are non-zero. Verify all travel/clearance z
    // heights are non-zero

    let tmp_dir = TempDir::create()?;
    let export = KicadPCBExport::generate(&args.board_path, tmp_dir.path())?;

    let mut layers = vec![];

    layers.push(PCBLayer {
        path: export.edge_cuts,
        usage: PCBLayerUsage::EdgeCuts,
        side: PCBLayerSide::All
    });

    layers.push(PCBLayer {
        path: export.drill_file,
        usage: PCBLayerUsage::Drill,
        side: PCBLayerSide::All
    });

    layers.push(PCBLayer {
        path: export.front_copper,
        usage: PCBLayerUsage::Copper,
        side: PCBLayerSide::Front
    });
    layers.push(PCBLayer {
        path: export.back_copper,
        usage: PCBLayerUsage::Copper,
        side: PCBLayerSide::Back
    });

    layers.push(PCBLayer {
        path: export.front_mask,
        usage: PCBLayerUsage::Mask,
        side: PCBLayerSide::Front
    });
    layers.push(PCBLayer {
        path: export.back_mask,
        usage: PCBLayerUsage::Mask,
        side: PCBLayerSide::Back
    });

    layers.push(PCBLayer {
        path: export.front_paste,
        usage: PCBLayerUsage::Paste,
        side: PCBLayerSide::Front
    });
    layers.push(PCBLayer {
        path: export.back_paste,
        usage: PCBLayerUsage::Paste,
        side: PCBLayerSide::Back
    });

    let options = PCBProcessorOptions {
        config,
        layers,
    };

    // TODO: For all isolation passes, inset inward all holes and on't do any isolation in the holes to save time

    // TODO: Need to verify that we well handle when contours get very small (e.g.
    // attempting to offline a circle/obround with close to its diameter may
    // collapse to a point or a line).

    // TODO: Warn when there is a solder mask hole for every pad labeled in the
    // copper layer.

    let start = Instant::now();
    let processor = PCBProcessor::create(options).await?;

    let program = match args.mode {
        Mode::SingleSidedFront => {
            processor.build_single_side_program(PCBLayerSide::Front)?
        }
        Mode::SingleSidedBack => {
            processor.build_single_side_program(PCBLayerSide::Back)?
        }
        Mode::LaserStencilFront => {
            processor.build_laser_stencil_program(PCBLayerSide::Front)?
        }
        Mode::LaserStencilBack => {
            processor.build_laser_stencil_program(PCBLayerSide::Back)?
        }
        Mode::DoubleSidedFront => {
            processor.build_double_sided_front_program()?
        }
        Mode::DoubleSidedBack { alignment_data } => {
            let mut data = SideAlignmentData::default();
            protobuf::text::parse_text_proto(&file::read_to_string(alignment_data).await?, &mut data)?;

            processor.build_double_sided_back_program(&data)?
        }
        _ => todo!()
    };

    let end = Instant::now();

    println!("Processing Time: {:?}", end - start);

    file::write(&args.output_path, &program).await?;

    Ok(())
}