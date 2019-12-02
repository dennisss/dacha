extern crate glfw;
extern crate gl;

use glfw::{Action, Context, Key};

fn main() {
	let mut glfw_inst = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();

	glfw_inst.window_hint(glfw::WindowHint::ContextVersion(3, 2));
	glfw_inst.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
	glfw_inst.window_hint(glfw::WindowHint::OpenGlProfile(
		glfw::OpenGlProfileHint::Core));


//	glfw_inst.window

	let (mut window, events) = glfw_inst.create_window(300, 300, "Hello this is window", glfw::WindowMode::Windowed)
		.expect("Failed to create GLFW window.");

	let (mut window2, events2) = glfw_inst.create_window(300, 300, "Hello this is window", glfw::WindowMode::Windowed)
		.expect("Failed to create GLFW window.");


	window.set_key_polling(true);

	gl::load_with(|s| window.get_proc_address(s) as *const _);

	/*
		Default opengl mode:
		- -1 to 1 in all dimensions
		- Step 1: normalize to 0 to width and 0 to height (top-left corner is (0,0))
		- Step 2: Assume z is 0 for now (we will keep around z functionality to
		  enable easy switching to 3d)
		- 

		TODO: Premultiply proj by modelview for each object?
	*/


	/*
	while !window.should_close() {
		
		window.make_current();

		unsafe {
			gl::ClearColor(255.0, 255.0, 255.0, 255.0);
			// 	glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);

			gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
		}

		window.swap_buffers();

		window2.make_current();

		unsafe {
			gl::ClearColor(0.0, 255.0, 255.0, 255.0);
			// 	glClear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT);

			gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
		}

		window2.swap_buffers();

		glfw_inst.poll_events();
		for (_, event) in glfw::flush_messages(&events) {
			handle_window_event(&mut window, event);
		}
	}
	*/
}

