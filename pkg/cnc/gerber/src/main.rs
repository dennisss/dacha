#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

use std::time::Instant;

use base_error::*;
use gerber::{
    excellon,
    graphics::{FillMode, GraphicsObject},
    processor::{CommandsProcessor, CommandsProcessorOptions},
};
use graphics::{
    canvas::{Paint, Path, PathBuilder},
    opengl::{canvas::OpenGLCanvas, canvas_render_loop::CanvasFrameHandler},
    raster::canvas_render_loop::WindowOptions,
    raster::canvas::RasterCanvas,
};
use math::geometry::{
    bounding_box::BoundingBoxBuilder,
    half_edge::{FaceDebug, HalfEdgeStruct},
};
use graphics::canvas::CanvasHelperExt;
use image::Color;

#[derive(Args)]
struct Args {
    #[arg(default = 0.0)]
    offset: f32,
}

struct Viewer {
    dirty: bool,
    objects: Vec<GraphicsObject>,
    scale: f32,
    translation: (f32, f32),
    // stroke_path: Path,

    fill_paths: Vec<Path>,

    path_i: usize,

}

impl CanvasFrameHandler for Viewer {
    fn render(
        &mut self,
        canvas: &mut dyn graphics::canvas::Canvas,
        window: &mut graphics::opengl::window::Window,
        events: &[graphics::glfw::WindowEvent],
    ) -> Result<()> {
        for e in events {
            if let glfw::WindowEvent::Key(key, _, glfw::Action::Press, _) = e {
                self.path_i += 1;
                self.dirty = true;
                println!("Press: {}", self.path_i % self.fill_paths.len());
            }
        }


        if !self.dirty {
            return Ok(());
        }


        canvas.clear_rect(
            0.,
            0.,
            window.width() as f32,
            window.height() as f32,
            &Color::rgb(0, 0, 0),
        )?;

        /*
        Transform steps:
        - Subtract minimum from coordinates
        - Multiple by -1 in the Y dimension
        - Scale by scaling factor (same in both dimensions).
        - Translate Y origin to the bottom of the window.
        */

        canvas.save();
        
        canvas.translate(0.0, window.height() as f32);
        canvas.scale(self.scale, -1.0 * self.scale);
        canvas.translate(self.translation.0, self.translation.1);

        /*
        for obj in &self.objects {
            for path in &obj.paths {
                if let FillMode::Dark = path.fill {
                    //
                } else {
                    // TODO:
                    println!("Non dark");
                    continue;
                }

                let mut o = canvas.create_path_fill(&path.path)?;

                o.draw(&Paint::color(image::Color::hex(0xffffff)), canvas)?;
            }

            // TODO: Handle separate layers.
        }
        */

        for path in &self.fill_paths {
            let mut o = canvas.create_path_fill(path)?;
            o.draw(&Paint::color(image::Color::hex(0xffffff)), canvas)?;
        }


        let i = self.path_i % self.fill_paths.len();

        // for path in &self.fill_paths {
            let mut o = canvas.create_path_fill(&self.fill_paths[i])?;
            o.draw(&Paint::color(image::Color::hex(0xff0000)), canvas)?;
        // }

        // let i = self.path_i % self.objects.len();
        // let obj = &self.objects[i];
        // for path in &obj.paths {
        //     let mut o = canvas.create_path_fill(&path.path)?;
        //     o.draw(&Paint::color(image::Color::hex(0x00ff00)), canvas)?;
        // }



        //

        println!("Paint done!");

        self.dirty = false;

        canvas.restore();

        Ok(())
    }
}

async fn read_drill_data() -> Result<()> {
    let data = file::read(project_path!(
        "pkg/cnc/boards/usb_power_switch/plot/usb_power_switch.drl"
    ))
    .await?;

    let file = excellon::DrillFile::parse_excellon(&data)?;

    println!("{:#?}", file);

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    // return read_drill_data().await;
    /*
    cargo run --bin kicad_export -- --board_path=pkg/cnc/boards/smart_servo/board/board.kicad_pcb --output_dir=/tmp/servo_gerber

    cargo run --bin gerber



    */


    let args = common::args::parse_args::<Args>()?;

    let data = file::read(project_path!(
        "/tmp/servo_gerber/board-F_Cu.gbr"
        // "pkg/cnc/boards/usb_power_switch/plot/usb_power_switch-Edge_Cuts.gbr"
    ))
    .await?;

    let f = gerber::syntax::File::parse(&data)?;

    let mut processor = CommandsProcessor::create(CommandsProcessorOptions {
        min_feature_size: 0.025,
    })?;

    let mut objs = vec![];

    for cmd in f.commands {
        processor.process(&cmd, &mut objs)?;
    }

    let mut bbox_builder = BoundingBoxBuilder::new();

    for obj in &objs {
        for path in &obj.paths {
            path.path.bbox_to(&mut bbox_builder);
        }
    }

    let bbox = bbox_builder.build();

    const WINDOW_SIZE: usize = 800;
    const MARGIN_MM: f32 = 4.0;

    let aspect_ratio = (2.0 * MARGIN_MM + bbox.max.y() - bbox.min.y())
        / (2.0 * MARGIN_MM + bbox.max.x() - bbox.min.x());

    let (mut height, mut width) = (if aspect_ratio > 1.0 {
        (WINDOW_SIZE, ((WINDOW_SIZE as f32) / aspect_ratio) as usize)
    } else {
        (((WINDOW_SIZE as f32) * aspect_ratio) as usize, WINDOW_SIZE)
    });

    let window_options = WindowOptions::new("Gerber Viewer", width, height);

    let mut half_edges = {
        let mut half_edges = HalfEdgeStruct::<bool>::new();

        for obj in objs.iter() {
            for path in &obj.paths {
                if let FillMode::Dark = path.fill {
                    //
                } else {
                    // TODO:
                    println!("Non dark");
                    continue;
                }

                let (vertices, path_starts) = path.path.linearize(0.025);
                for i in 0..(path_starts.len() - 1) {
                    let start_i = path_starts[i];
                    let end_i = path_starts[i + 1];
                    half_edges.add_face(true, vertices[start_i..end_i].iter().cloned());
                }
            }
        }

        half_edges.repair();
        half_edges.merge_faces();

        half_edges

    };

    let fill_paths = {
        /*
        let mut half_edges = HalfEdgeStruct::<bool>::new();

        let mut num_objects = 0;

        /*
        --i=17 is weird

        42
        */

        // TODO: Dedup this a lot.
        println!("Num objects: {}", objs.len());
        for (i, obj) in objs.iter().enumerate() {
            
            let keep = obj.is_region || i == args.i;
            if !keep {
                continue;
            }
            println!("dRAW REGION");

            for path in &obj.paths {
                if let FillMode::Dark = path.fill {
                    //
                } else {
                    // TODO:
                    println!("Non dark");
                    continue;
                }

                num_objects += 1;

                let (vertices, path_starts) = path.path.linearize(0.025);
                for i in 0..(path_starts.len() - 1) {
                    let start_i = path_starts[i];
                    let end_i = path_starts[i + 1];
                    half_edges.add_face(true, vertices[start_i..end_i].iter().cloned());
                }
            }

            // match obj {

            //     GraphicsObject::FillPath(path, fill_mode) => {

            //         // let (vertices, path_starts) =

            //         // half_edges
            //     }
            //     GraphicsObject::Line(_, _) => todo!(),
            //     // TODO:
            //     GraphicsObject::EndOfLayer => {}
            // }
        }
        */

        // println!("NUM TO DRAW: {}", num_objects);

        let t1 = Instant::now();

        // half_edges.repair();

        // half_edges.merge_faces();

        // let t2 = Instant::now();

        if args.offset != 0.0 {
            half_edges = math::geometry::offsetting::offset_faces(&half_edges, args.offset, 0.025);
            half_edges.merge_faces();
        }


        // let t22 = Instant::now();

        // half_edges.merge_faces();

        // let t3 = Instant::now();

        // println!("{:?} : {:?} : {:?}", t2 - t1, t22 - t2, t3 - t22);

        let faces = FaceDebug::get_all(&half_edges);

        println!("NUM FACES: {}", faces.len());
        println!("Half Edges: {}", half_edges.num_half_edges());

        let mut fill_paths = vec![];


        for face in faces {
            // if face.outer_component.is_some() {
            //     continue;
            // }

            if !face.label {
                // println!("HAVE LABEL");
                continue;
            }

            let mut path_builder = PathBuilder::new();

            // println!("DRAW FACE");

            for boundary in face
                .outer_component
                .iter()
                .chain(face.inner_components.iter())
            {
                path_builder.move_to(boundary[0].clone());
                for p in &boundary[1..] {
                    path_builder.line_to(p.clone());
                }
                path_builder.close();
            }

            fill_paths.push(path_builder.build());

            // Take all the outer and inner components. Offset them all to the
            // right.

            /*
            We may be intersecting with the previous segment (in that case, clip)

            */

            // println!("{:?}", face);
        }

        fill_paths
    };

    /*
    Every boundary is a path to trace out.

    - For now, the assumption is that we have


    */

    println!("Start to render!");

    let mut viewer = Viewer {
        dirty: true,
        objects: objs,
        translation: (MARGIN_MM - bbox.min.x(), MARGIN_MM - bbox.min.y()),
        scale: (height as f32) / (2.0 * MARGIN_MM + bbox.max.y() - bbox.min.y()),
        fill_paths,
        path_i: 0,
    };

    

    let mut canvas = RasterCanvas::create(height, width);
    canvas.render_loop(window_options, |mut canvas, window, events| {
        viewer.render(canvas, window, events)
    }).await?;


    // OpenGLCanvas::render_loop(window_options, viewer).await?;

    /*
    - Get everything into a half-edge datastructure and find all the non-filled countours.

    - All boudnaries of faces with no or dark label are the ones that we want to outline.

    - Generate the offset lines.
    - Put the offset lines back into the half-edge struct
    - extract the appropriate contours.
    -

    */

    Ok(())
}
