use gl::types::{GLint, GLuint};
use math::matrix::*;
use std::ptr::null;

// Safer wrappers around storing matrices in OpenGL.

pub fn gl_uniform_vec3(location: GLuint, value: &Vector3f) {
	unsafe { gl::Uniform3fv(location as GLint, 1, value.as_ptr()); }
}

pub fn gl_uniform_vec4(location: GLuint, value: &Vector4f) {
	unsafe { gl::Uniform4fv(location as GLint, 1, value.as_ptr()); }
}

pub fn gl_uniform_mat4(location: GLuint, value: &Matrix4f) {
	unsafe {
		gl::UniformMatrix4fv(location as GLint, 1, gl::TRUE, value.as_ptr());
	}
}


/// Creates a new buffer copying the given vector into graphics memory and
/// binding it to the given attribute.
pub fn gl_vertex_buffer_vec3(attr: GLuint, data: &[Vector3f]) -> GLuint {
	let mut buf = 0;
	unsafe {
		gl::GenBuffers(1, &mut buf);
		gl::BindBuffer(gl::ARRAY_BUFFER, buf);
		gl::BufferData(gl::ARRAY_BUFFER,
					   (std::mem::size_of::<Vector3f>() * data.len()) as isize,
					   std::mem::transmute(data.as_ptr()),
					   gl::STATIC_DRAW);
		gl::VertexAttribPointer(attr, 3, gl::FLOAT, gl::FALSE, 0, null());
	}
	buf
}
