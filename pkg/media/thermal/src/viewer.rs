use std::sync::Arc;

use common::errors::*;
use image::Image;
use graphics::opengl::canvas::OpenGLCanvas;
use graphics::raster::canvas_render_loop::WindowOptions;
use executor::child_task::ChildTask;
use graphics::opengl::canvas_render_loop::CanvasFrameHandler;
use executor::sync::SyncMutex;
use graphics::canvas::Paint;

const SCALE: usize = 4;

pub struct Viewer {
    shared: Arc<Shared>,
    task: ChildTask,
}

struct Shared {
    // TODO: Use an async way of communicating with the thread.
    state: SyncMutex<State>
}

struct State {
    // The next image to display if any new one is available.
    image: Option<Image<u8>>,
}

impl Viewer {
    pub fn create() -> Result<Self> {

        let shared = Arc::new(Shared {
            state: SyncMutex::new(State {
                image: None
            })
        });

        let task = ChildTask::spawn(Self::run_window(shared.clone()));

        Ok(Self {
            shared,
            task,
        })
    }

    pub fn set_image(&self, image: Image<u8>) {
        self.shared.state.apply(|state| {
            state.image = Some(image);
        }).unwrap();
    }

    async fn run_window(shared: Arc<Shared>) {
        // TODO: Have better tracking of this.
        if let Err(e) = Self::run_window_inner(&shared).await {
            eprintln!("Window failed: {}", e);
        }
    }

    async fn run_window_inner(shared: &Arc<Shared>) -> Result<()> {
        let window_options = WindowOptions::new("Thermal Viewer", 256*SCALE, 192*SCALE);

        OpenGLCanvas::render_loop(window_options, ViewerHandler { shared: shared.clone() })
        .await
    }
}

struct ViewerHandler {
    shared: Arc<Shared>
}

impl CanvasFrameHandler for ViewerHandler {
    fn render(
        &mut self,
        canvas: &mut dyn graphics::canvas::Canvas,
        window: &mut graphics::opengl::window::Window,
        events: &[graphics::glfw::WindowEvent],
    ) -> Result<()> {
        let img = match self.shared.state.apply(|s| s.image.take())? {
            Some(v) => v,
            None => return Ok(())
        };

        canvas.save();
        canvas.scale(SCALE as f32, SCALE as f32);

        let mut obj = canvas.create_image(&img)?;
        obj.draw(&Paint::color(image::Color::hex(0xffffff)), canvas)?;

        canvas.restore();

        Ok(())
    }
}