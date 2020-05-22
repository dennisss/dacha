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

pub struct GLBuffer {
	pub id: GLuint
}

impl Drop for GLBuffer {
	fn drop(&mut self) {
		unsafe { gl::DeleteBuffers(1, &self.id); }
	}
}


/// Creates a new buffer copying the given vector into graphics memory and
/// binding it to the given attribute.
pub fn gl_vertex_buffer_vec3(attr: GLuint, data: &[Vector3f]) -> GLBuffer {
	let mut buf = 0;
	unsafe {
		gl::GenBuffers(1, &mut buf);
		gl::BindBuffer(gl::ARRAY_BUFFER, buf);
		gl::BufferData(gl::ARRAY_BUFFER,
					   (std::mem::size_of::<Vector3f>() * data.len()) as isize,
					   std::mem::transmute(data.as_ptr()), gl::STATIC_DRAW);
		gl::VertexAttribPointer(attr, 3, gl::FLOAT, gl::FALSE, 0, null());
	}

	GLBuffer { id: buf }
}

pub fn gl_indices_buffer(data: &[GLuint]) -> GLBuffer {
	let mut buf = 0;
	unsafe {
		gl::GenBuffers(1, &mut buf);
		gl::BindBuffer(gl::ELEMENT_ARRAY_BUFFER, buf);
		gl::BufferData(gl::ELEMENT_ARRAY_BUFFER,
					   (std::mem::size_of::<GLuint>() * data.len()) as isize,
					   std::mem::transmute(data.as_ptr()), gl::STATIC_DRAW);
	}

	GLBuffer { id: buf }
}

/// Special case of above for triangle faces
pub fn gl_face_buffer(data: &[[GLuint; 3]]) -> GLBuffer {
	// TODO: Will this have the correct size
	// std::slice::from_raw_parts(data.as_ptr(), 3*data.len())
	unsafe { gl_indices_buffer(std::mem::transmute(data)) }
}
