use crate::window::Window;
use glfw::{Action, Context, Key};
use math::matrix::Vector2i;
use std::sync::Arc;
use std::sync::Mutex;

/// Top-level context for a graphical application. Manages all open windows.
pub struct Application {
    glfw_inst: glfw::Glfw,
    windows: Vec<Arc<Mutex<Window>>>,
}

impl Application {
    pub fn new() -> Self {
        let mut glfw_inst = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
        glfw_inst.window_hint(glfw::WindowHint::ContextVersion(3, 2));
        glfw_inst.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        glfw_inst.window_hint(glfw::WindowHint::OpenGlProfile(
            glfw::OpenGlProfileHint::Core,
        ));
        glfw_inst.window_hint(glfw::WindowHint::Resizable(false));

        // TODO: Ensure RGBA with depth buffer

        Self {
            glfw_inst,
            windows: vec![],
        }
    }

    pub fn create_window(
        &mut self,
        name: &str,
        size: &Vector2i,
        visible: bool,
    ) -> Arc<Mutex<Window>> {
        self.glfw_inst
            .window_hint(glfw::WindowHint::Visible(visible));

        // TODO: http://www.glfw.org/docs/latest/context_guide.html is very useful with documenting how to do off screen windows and windows that share context with other windows
        let (mut window, events) = self
            .glfw_inst
            .create_window(
                size.x() as u32,
                size.y() as u32,
                name,
                glfw::WindowMode::Windowed,
            )
            .expect("Failed to create GLFW window.");

        window.set_key_polling(true);

        gl::load_with(|s| window.get_proc_address(s) as *const _);

        window.make_current();
        self.glfw_inst
            .set_swap_interval(glfw::SwapInterval::Sync(1));

        unsafe {
            gl::Enable(gl::DEPTH_TEST);
            gl::DepthFunc(gl::LEQUAL);
        }

        let w = Arc::new(Mutex::new(Window::from(window, events)));
        self.windows.push(w.clone());
        w
    }

    pub fn run(&mut self) {
        loop {
            let mut some_open = false;

            self.glfw_inst.poll_events();
            for w in &self.windows {
                let mut w = w.lock().unwrap();
                if w.window.should_close() {
                    // TODO: Remove from array.
                    continue;
                }

                some_open = true;

                w.tick();
                w.draw();
            }

            if !some_open {
                break;
            }
        }
    }
}
