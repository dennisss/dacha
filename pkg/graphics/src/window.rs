use crate::drawable::Drawable;
use crate::group::Group;
use crate::transform::Camera;
use crate::transform::Transform;
use glfw::Context;
use math::matrix::{Vector2i, Vector4f};
use std::sync::mpsc::Receiver;

/// Represents a drawing space either linked to a whole window or a viewport
pub struct Window {
    pub scene: Group,
    pub camera: Camera,
    //	pub size: Vector2i, // TODO: Use unsigned
    background_color: Vector4f,

    pub window: glfw::Window,
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

    /// Size is width x height
    pub fn set_size(&mut self, size: Vector2i) {
        // glfwSetWindowSize(this->window, size.x, size.y);
        self.window.set_size(size[0] as i32, size[1] as i32);
    }

    fn handle_window_event(window: &mut glfw::Window, event: glfw::WindowEvent) {}

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

    inline void setBackgroundColor(glm::vec4 color){ this->backgroundColor = color; };

};

// All defined windows; The index corresponds to the id
static int nWindows = 0;
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




Window::Window(GLFWwindow *window) {
    this->window = window;

    this->backgroundColor = vec4(0.0f, 0.0f, 0.0f, 1.0f); // Default color of black
}

Window::~Window(){
    glfwDestroyWindow(this->window);

    nWindows--;
    if(nWindows == 0) {
        glfwTerminate();
    }
}


void Window::run() {
    while(!glfwWindowShouldClose(this->window)) {

        this->draw();

        glfwPollEvents(); // NOTE: This applies to all open windows
    }
}

static void error_callback(int error, const char* description) {
    printf("%s\n", description);
}


*/
