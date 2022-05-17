use std::sync::Arc;

use common::errors::*;
use image::Image;
use math::matrix::{Vector2f, Vector2i, Vector3f};

use crate::app::Application;
use crate::polygon::Polygon;
use crate::shader::{Shader, ShaderSource};
use crate::texture::Texture;
use crate::transform::orthogonal_projection;

const MAX_DIMENSION: f32 = 1000.0;

#[async_trait]
pub trait ImageShow {
    // TODO: Spawn this on a separate thread so that it doesn't block.
    async fn show(&self) -> Result<()>;
}

#[async_trait]
impl ImageShow for Image<u8> {
    async fn show(&self) -> Result<()> {
        let (window_width, window_height) = {
            let aspect_ratio = (self.width() as f32) / (self.height() as f32);

            if self.width() < self.height() {
                (
                    (aspect_ratio * MAX_DIMENSION).round() as isize,
                    MAX_DIMENSION as isize,
                )
            } else {
                (
                    MAX_DIMENSION as isize,
                    (MAX_DIMENSION / aspect_ratio).round() as isize,
                )
            }
        };

        let shader_Src = ShaderSource::flat_texture().await?;

        let mut app = Application::new();
        let mut window = app.create_window(
            "Image",
            Vector2i::from_slice(&[window_width, window_height]),
            true,
        );

        let shader = Arc::new(shader_Src.compile().unwrap());

        let texture = Arc::new(Texture::new(self));
        let mut rect = Polygon::rectangle(
            Vector2f::from_slice(&[0.0, 0.0]),
            window_width as f32,
            window_height as f32,
            Vector3f::from_slice(&[1.0, 1.0, 0.0]),
            shader,
        );

        // y coordinates are multiplied by -1 because our projection matrix flips along
        // y.
        rect.set_texture(
            texture,
            &[
                Vector2f::from_slice(&[0.0, -1.0]),
                Vector2f::from_slice(&[1.0, -1.0]),
                Vector2f::from_slice(&[1.0, 0.0]),
                Vector2f::from_slice(&[0.0, 0.0]),
            ],
        );

        window.camera.proj = orthogonal_projection(
            0.0,
            window_width as f32,
            window_height as f32,
            0.0,
            -1.0,
            1.0,
        );

        window.scene.add_object(Box::new(rect));

        app.render_loop(move || {
            window.tick();
            window.draw();

            !window.raw().should_close()
        });

        Ok(())
    }
}
