use std::sync::mpsc::Receiver;

use glfw::Context;
use math::matrix::{Vector2i, Vector4f};

use crate::opengl::drawable::Drawable;
use crate::opengl::group::Group;
use crate::transform::Camera;
use crate::transform::Transform;

/// Represents a drawing space either linked to a whole window or a viewport
///
/// TODO: Make all the fields private?
pub struct Window {
    pub scene: Group,
    pub camera: Camera,
    background_color: Vector4f,

    window: glfw::Window,
    events: Receiver<(f64, glfw::WindowEvent)>,
}

impl Window {
    pub fn from(window: glfw::Window, events: Receiver<(f64, glfw::WindowEvent)>) -> Self {
        Self {
            scene: Group::default(),
            camera: Camera::default(),

            // Default color is black.
            background_color: Vector4f::from_slice(&[0.0, 0.0, 0.0, 1.0]),
            window,
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

    pub fn draw(&mut self) {
        self.window.make_current();

        unsafe {
            let color = &self.background_color;
            gl::ClearColor(color.x(), color.y(), color.z(), color.w());
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        let base = Transform::from(self.camera.view.clone());
        self.scene.draw(&self.camera, &base);

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
