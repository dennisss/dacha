use alloc::rc::Rc;

use common::errors::*;
use glfw::WindowEvent;
use image::{Colorspace, Image};
use math::array::Array;
use math::matrix::{vec2f, Vector2f, Vector2i, Vector3f};

use crate::canvas::base::CanvasBase;
use crate::canvas::Canvas;
use crate::opengl::app::Application;
use crate::opengl::canvas::OpenGLCanvas;
use crate::opengl::drawable::Drawable;
use crate::opengl::framebuffer::FrameBuffer;
use crate::opengl::polygon::Polygon;
use crate::opengl::shader::ShaderSource;
use crate::opengl::texture::Texture;
use crate::opengl::window::Window;
use crate::transform::{orthogonal_projection, Camera, Transform};

pub use crate::raster::canvas_render_loop::WindowOptions;

/*
Why draw to a framebuffer instead of directly to the window?
- Doesn't require the screen to be configured with depth/render buffers.
- More generic solution to implement MSAA
- We can make incremental updates to the draw buffer while still supporting swap_buffer of the main window.
*/

impl OpenGLCanvas {
    /// TODO: The callback function must not store any opengl objects outside of
    /// the function. Otherwise we can't destroy the window until all objects
    /// are destroyed.
    pub async fn render_loop<
        F: FnMut(&mut dyn Canvas, &mut Window, &[WindowEvent]) -> Result<()>,
    >(
        window_options: WindowOptions,
        mut f: F,
    ) -> Result<()> {
        let shader_src = ShaderSource::simple().await?;

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

        let shader = Rc::new(shader_src.compile(&mut window).unwrap());

        let mut frame_buffer = FrameBuffer::new(
            window.context(),
            window_options.width * 2,
            window_options.height * 2,
        )?;

        let mut camera = Camera::default();
        camera.proj = orthogonal_projection(
            0.0,
            window_options.width as f32,
            window_options.height as f32,
            0.0,
            -1.0,
            1.0,
        );

        let image = Image::<u8> {
            array: Array {
                shape: vec![1, 1, 3],
                data: vec![255, 255, 255],
            },
            colorspace: Colorspace::RGB,
        };

        let empty_texture = Rc::new(Texture::new(window.context(), &image));

        let mut canvas = OpenGLCanvas {
            base: CanvasBase::new(),
            camera,
            shader,
            empty_texture,
            context: window.context(),
        };

        // TODO: Run on a separate thread to avoid blocking the async threads.
        app.render_loop(|| {
            // TODO: Support logging the frame rate of this.

            events.clear();
            for (_, e) in window.received_events() {
                events.push(e);
            }

            // TODO: Return this error from the outer function.

            window.begin_draw();

            frame_buffer.draw_context(|| {
                unsafe {
                    gl::Viewport(
                        0,
                        0,
                        (window_options.width * 2) as i32,
                        (window_options.height * 2) as i32,
                    )
                };

                f(&mut canvas, &mut window, &events).unwrap();
            });

            unsafe {
                gl::Viewport(
                    0,
                    0,
                    window_options.width as i32,
                    window_options.height as i32,
                )
            };

            // TODO: Cache this rectangle across draws.
            let mut rect = Polygon::rectangle(vec2f(-1., -1.), 2., 2., canvas.shader.clone());
            rect.set_texture(frame_buffer.texture())
                .set_vertex_colors(Vector3f::from_slice(&[1., 1., 1.]))
                .set_vertex_alphas(1.);

            rect.draw(&Camera::default(), &Transform::default());

            window.end_draw();

            !window.raw().should_close()
        });

        Ok(())
    }
}
