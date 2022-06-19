use alloc::rc::Rc;
use core::cell::RefCell;
use std::sync::mpsc::Receiver;

use glfw::Context;
use math::matrix::{Vector2i, Vector4f};

use crate::opengl::drawable::Drawable;
use crate::opengl::group::Group;
use crate::transform::Camera;
use crate::transform::Transform;

#[derive(Clone)]
pub struct WindowContext {
    render_context: Rc<RefCell<glfw::RenderContext>>,
}

impl WindowContext {
    pub fn make_current(&mut self) {
        // TODO: Make this require a mutex lock as only one thread can have a window
        // active at a given time.
        // Although we do want to enable nested usage of it.
        self.render_context.borrow_mut().make_current();
    }
}

/// Represents a drawing space either linked to a whole window or a viewport
///
/// TODO: Make all the fields private?
pub struct Window {
    pub scene: Group,
    pub camera: Camera,
    background_color: Vector4f,

    window: glfw::Window,
    context: WindowContext,

    events: Receiver<(f64, glfw::WindowEvent)>,
}

impl Window {
    pub fn from(mut window: glfw::Window, events: Receiver<(f64, glfw::WindowEvent)>) -> Self {
        let context = WindowContext {
            render_context: Rc::new(RefCell::new(window.render_context())),
        };

        Self {
            scene: Group::default(),
            camera: Camera::default(),

            // Default color is black.
            background_color: Vector4f::from_slice(&[0.0, 0.0, 0.0, 1.0]),
            window,
            context,
            events,
        }
    }

    pub fn raw(&mut self) -> &mut glfw::Window {
        &mut self.window
    }

    /// Size is width x height
    pub fn set_size(&mut self, size: Vector2i) {
        self.window.set_size(size[0] as i32, size[1] as i32);
    }

    pub fn width(&self) -> usize {
        self.window.get_size().0 as usize
    }

    pub fn height(&self) -> usize {
        self.window.get_size().1 as usize
    }

    pub fn received_events<'a>(
        &'a mut self,
    ) -> impl Iterator<Item = (f64, glfw::WindowEvent)> + 'a {
        glfw::flush_messages(&self.events)
    }

    pub fn tick(&mut self) {
        for (_, event) in glfw::flush_messages(&self.events) {
            match event {
                glfw::WindowEvent::Key(glfw::Key::Escape, _, glfw::Action::Press, _) => {
                    self.window.set_should_close(true)
                }
                _ => {}
            }
        }
    }

    pub fn context(&mut self) -> WindowContext {
        self.context.clone()
    }

    pub fn begin_draw(&mut self) {
        self.window.make_current();
    }

    pub fn end_draw(&mut self) {
        self.window.swap_buffers();
    }

    pub fn draw(&mut self) {
        self.window.make_current();

        unsafe {
            let color = &self.background_color;
            gl::ClearColor(color.x(), color.y(), color.z(), color.w());
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        let base = Transform::from(self.camera.view.clone());
        self.scene.draw(&self.camera, &base);

        /*
        TODO: Use glCopyPixels to support copying from an intermediate source before swapping in the buffer (so we preserve the old buffer for incremental rendering).

        - In canvas rendering, typically we would use clear_rect() to do full or partial screen re-draws.
        */

        self.window.swap_buffers();
    }
}

/*

/* Represents a drawing space either linked to a whole window or a viewport */
class Window {
public:

    static Window *Create(const char *name, glm::vec2 size = glm::vec2(1280, 720), bool visible = true);

    void run();
    void draw();
    void setSize(glm::vec2 size);

};

/*
void window_reshape(int w, int h){
    int wid = glutGetWindow();
    Window *win = windows[wid];
    win->size = vec2(w, h);

    // TODO: Allow a fixed aspect ratio as in assignment 3
    float dim = fmin(w, h);
    glViewport((w - dim) / 2.0, (h - dim) / 2.0, dim, dim);
}
*/

static void error_callback(int error, const char* description) {
    printf("%s\n", description);
}


*/
