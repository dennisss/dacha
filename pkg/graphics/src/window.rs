use math::matrix::{Vector4f, Vector2i};
use std::sync::mpsc::Receiver;
use glfw::Context;
use crate::transform::Transform;
use crate::group::Group;
use crate::transform::Camera;
use crate::drawable::Drawable;

///// Number of currently open windows. GLFW will be automatically initialized or
///// terminated when this transitions to/from
//const NUM_WINDOWS: AtomicUsize = AtomicUsize::new(0);



/// Represents a drawing space either linked to a whole window or a viewport
struct Window {
	pub scene: Group,
	pub camera: Camera,
//	pub size: Vector2i, // TODO: Use unsigned

	background_color: Vector4f,

	window: glfw::Window,
	events: Receiver<(f64, glfw::WindowEvent)>,
}

impl Window {

	fn from(window: glfw::Window,
			events: Receiver<(f64, glfw::WindowEvent)>) -> Self {
		Self {
			scene: Group::default(),
			camera: Camera::default(),

			// Default color is black.
			background_color: Vector4f::from_slice(&[0.0, 0.0, 0.0, 1.0]),
			window, events
		}
	}

	/// Size is width x height
	pub fn set_size(&mut self, size: Vector2i) {
		// glfwSetWindowSize(this->window, size.x, size.y);
		self.window.set_size(size[0] as i32, size[1] as i32);
	}

	pub fn run(&mut self) {

	}

	fn handle_window_event(window: &mut glfw::Window, event: glfw::WindowEvent) {
		match event {
			glfw::WindowEvent::Key(glfw::Key::Escape, _,
								   glfw::Action::Press, _) => {
				window.set_should_close(true)
			}
			_ => {}
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


Window *Window::Create(const char *name, vec2 size, bool visible) {

	if(nWindows == 0) {
		if(!glfwInit()) {
			// Initialization failed
			printf("GLFW Failed to initialize\n");
		}

		glfwSetErrorCallback(error_callback);

		glfwWindowHint(GLFW_CONTEXT_VERSION_MAJOR, 3);
		glfwWindowHint(GLFW_CONTEXT_VERSION_MINOR, 2);
		glfwWindowHint(GLFW_OPENGL_FORWARD_COMPAT, GL_TRUE);
		glfwWindowHint(GLFW_OPENGL_PROFILE, GLFW_OPENGL_CORE_PROFILE);

		glfwWindowHint(GLFW_RESIZABLE, GL_FALSE);

		// TODO: Ensure RGBA with depth buffer
	}


	nWindows++;


	glfwWindowHint(GLFW_VISIBLE, visible? GLFW_TRUE : GLFW_FALSE);

	// TODO: http://www.glfw.org/docs/latest/context_guide.html is very useful with documenting how to do off screen windows and windows that share context with other windows
	GLFWwindow* window = glfwCreateWindow(size.x, size.y, name, NULL, NULL);

	glfwMakeContextCurrent(window);


	int res;
	glewExperimental = GL_TRUE;
	if((res = glewInit()) != GLEW_OK){
		fprintf(stderr, "GLEW Failed: %s\n", glewGetErrorString(res));
		return NULL;
	}


	glfwSwapInterval(1);


	glEnable(GL_DEPTH_TEST);
	glDepthFunc(GL_LEQUAL);

	Window *w = new Window(window);
	w->size = size;

	return w;
}




*/