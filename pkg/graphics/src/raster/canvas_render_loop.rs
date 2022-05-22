use std::sync::Arc;

use common::errors::*;
use glfw::WindowEvent;
use math::matrix::{Vector2f, Vector2i, Vector3f};

use crate::app::Application;
use crate::polygon::Polygon;
use crate::raster::canvas::Canvas;
use crate::shader::ShaderSource;
use crate::texture::Texture;
use crate::transform::orthogonal_projection;
use crate::window::Window;

pub struct WindowOptions {
    pub name: String,
    pub width: usize,
    pub height: usize,
}

impl Canvas {
    pub async fn render_loop<F: FnMut(&mut Canvas, &mut Window, &[WindowEvent]) -> Result<()>>(
        &mut self,
        window_options: WindowOptions,
        mut f: F,
    ) -> Result<()> {
        let shader_src = ShaderSource::flat_texture().await?;

        let mut app = Application::new();

        let mut window = app.create_window(
            &window_options.name,
            Vector2i::from_slice(&[
                window_options.width as isize,
                window_options.height as isize,
            ]),
            true,
        );

        let mut events = vec![];

        let shader = Arc::new(shader_src.compile().unwrap());

        window.camera.proj = orthogonal_projection(
            0.0,
            window_options.width as f32,
            window_options.height as f32,
            0.0,
            -1.0,
            1.0,
        );

        // TODO: Run on a separate thread to avoid
        app.render_loop(|| {
            // TODO: Support logging the frame rate of this.

            events.clear();
            for (_, e) in window.received_events() {
                events.push(e);
            }

            // TODO: Return this error from the outer function.
            f(self, &mut window, &events).unwrap();

            window.scene.clear();

            let texture = Arc::new(Texture::new(&self.drawing_buffer));
            let mut rect = Polygon::rectangle(
                Vector2f::from_slice(&[0.0, 0.0]),
                window_options.width as f32,
                window_options.height as f32,
                Vector3f::from_slice(&[1.0, 1.0, 0.0]),
                shader.clone(),
            );

            rect.set_texture(
                texture,
                &[
                    Vector2f::from_slice(&[0.0, -1.0]),
                    Vector2f::from_slice(&[1.0, -1.0]),
                    Vector2f::from_slice(&[1.0, 0.0]),
                    Vector2f::from_slice(&[0.0, 0.0]),
                ],
            );

            window.scene.add_object(Box::new(rect));

            // window.tick();
            window.draw();

            !window.raw().should_close()
        });

        Ok(())
    }
}
