use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

use common::async_std::channel;
use glfw::{Action, Context, Key};
use math::matrix::{Vector2i, Vector4f};

use crate::opengl::drawable::Drawable;
use crate::opengl::window::Window;

/// Top-level context for a graphical application. Manages all open windows.
///
/// NOTE: This may only live on one thread.
pub struct Application {
    glfw_inst: glfw::Glfw,
}

impl Application {
    pub fn new() -> Self {
        let mut glfw_inst = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();
        glfw_inst.window_hint(glfw::WindowHint::ContextVersion(3, 2));
        glfw_inst.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
        glfw_inst.window_hint(glfw::WindowHint::OpenGlProfile(
            glfw::OpenGlProfileHint::Core,
        ));

        glfw_inst.window_hint(glfw::WindowHint::DepthBits(Some(0)));
        glfw_inst.window_hint(glfw::WindowHint::AlphaBits(Some(0)));
        // glfw_inst.window_hint(glfw::WindowHint::Samples(Some(4)));

        // TODO: Ensure RGBA with depth buffer

        Self { glfw_inst }
    }

    pub fn create_window(
        &mut self,
        name: &str,
        size: Vector2i,
        visible: bool,
        resizable: bool,
    ) -> Window {
        self.glfw_inst
            .window_hint(glfw::WindowHint::Visible(visible));
        self.glfw_inst
            .window_hint(glfw::WindowHint::Resizable(resizable));

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
        window.set_cursor_pos_polling(true);
        window.set_cursor_enter_polling(true);
        window.set_mouse_button_polling(true);
        window.set_char_mods_polling(true);

        gl::load_with(|s| window.get_proc_address(s) as *const _);

        window.make_current();
        self.glfw_inst
            .set_swap_interval(glfw::SwapInterval::Sync(1));

        unsafe {
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            // gl::Enable(gl::DEPTH_TEST);
            // gl::DepthFunc(gl::LEQUAL);

            // gl::Enable(gl::MULTISAMPLE);
        }

        Window::from(window, events)
    }

    pub fn render_loop<F: FnMut() -> bool>(&mut self, mut f: F) {
        let start_time = std::time::Instant::now();
        let frame_time = std::time::Duration::from_secs_f32(1. / 30.);

        loop {
            self.glfw_inst.poll_events();

            if !f() {
                break;
            }
            // TODO: Ideally if any events weren't read, we would clear the
            // receivers?

            // TODO: Make this account for the amount of time spent.

            std::thread::sleep(frame_time);
        }
    }

    /*
    pub fn poll_events(&mut self) {
        self.glfw_inst.poll_events();
    }
    */

    /*
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
    */
}
