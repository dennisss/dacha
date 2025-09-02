

#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::time::Instant;
use std::f32::consts::PI;

use base_error::*;
// use cnc_monitor::program::new_progress_tracker;
use common::io::Writeable;
use file::LocalPathBuf;
use math::matrix::Vector2f;
use graphics::{
    canvas::{Canvas, CanvasHelperExt, Paint},
    font::{CanvasFontRenderer, FontStyle, OpenTypeFont, TextAlign, VerticalAlign},
    image_show::ImageShow,
    raster::canvas::RasterCanvas,
};
use graphics::canvas::PathBuilder;
use image::{BinaryImage, Color, Image};

// #[derive(Args)]
// struct Args {
//     #[arg(positional)]
//     path: LocalPathBuf,

//     preset: String,

//     output_dir: LocalPathBuf,
// }

fn draw_circle(center_x: f32, center_y: f32, r: f32, canvas: &mut dyn Canvas) -> Result<()> {
    let mut path = PathBuilder::new();
    path.ellipse(
        Vector2f::from_slice(&[center_x, center_y]),
        Vector2f::from_slice(&[r, r]),
        0.0,
        2.0 * PI,
    );

    canvas.create_path_fill(&path.build())?.draw(
        &Paint {
            color: Color::rgb(0, 0, 0),
            alpha: 1.,
        },
        canvas,
    )
}

pub struct CameraCalibrationPattern {
    pub num_cols: usize,
    pub num_rows: usize,
    pub circle_radius: f32,
    pub col_spacing: f32,
    pub row_spacing: f32
}

#[executor_main]
async fn main() -> Result<()> {

    let mut canvas = RasterCanvas::create(1000, 1000);
    let c = &mut canvas as &mut dyn Canvas;
    c.clear_rect(
        0.,
        0.,
        1000.,
        1000.,
        &Color::rgb(255, 255, 255),
    )?;

    let pattern = CameraCalibrationPattern {
        num_cols: 7,
        num_rows: 4,
        circle_radius: 10.0,
        col_spacing: 30.0,
        row_spacing: 30.0
    };

    for row in 0..pattern.num_rows {
        for col in 0..pattern.num_cols {
            // Asymetric pattern.
            if (row + col) % 2 != 0 {
                continue;
            }

            let x = ((col as f32) + 0.5) * pattern.col_spacing;
            let y = ((row as f32) + 0.5) * pattern.row_spacing;

            draw_circle(x, y, pattern.circle_radius, c)?;
        }
    }







    // out.push(GraphicsPath {
    //     path: path_builder.build(),
    //     fill: exposure,
    // });

    canvas.drawing_buffer.show().await?;


    Ok(())
}