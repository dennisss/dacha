pub mod app;
pub mod canvas;
pub mod canvas_render_loop;
pub mod drawable;
pub mod framebuffer;
pub mod group;
pub mod mesh;
pub mod object;
pub mod polygon;
pub mod shader;
mod shader_attributes;
pub mod texture;
mod util;
pub mod window;

use common::errors::*;
use image::Color;
use math::matrix::vec2f;

use crate::canvas::*;
use crate::opengl::canvas::OpenGLCanvas;
use crate::opengl::canvas_render_loop::WindowOptions;

pub async fn run() -> Result<()> {
    let window_options = WindowOptions::new("OpenGL Canvas!", 800, 600);

    OpenGLCanvas::render_loop(window_options, |canvas, window, events| {
        //

        let mut path = PathBuilder::new();
        path.move_to(vec2f(100., 100.));
        path.line_to(vec2f(500., 100.));
        path.line_to(vec2f(300., 500.));
        path.close();

        canvas.save();

        // canvas.translate(400., 300.);
        // canvas.scale(100., 100.);

        canvas.fill_path(&path.build(), &Color::rgb(255, 0, 0));

        canvas.restore();

        // canvas.fill_path

        Ok(())
    })
    .await
}
