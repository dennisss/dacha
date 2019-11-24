use common::errors::*;
use gl::types::{GLuint, GLsizei, GLint, GLenum};
use std::ptr::null;
use std::ffi::CStr;
use math::matrix::Matrix4f;
use async_std::fs::File;
use async_std::io::Read;
use crate::util::*;
use crate::lighting::{Material, LightSource, MAX_LIGHTS};

const MAX_ERROR_LENGTH: GLsizei = 2048;


pub struct Shader {
	pub program: GLuint,
	pub pos_attrib: GLuint,
	pub normal_attrib: GLuint,
	pub color_attrib: GLuint,

	pub uni_proj_attrib: GLuint,
	pub uni_modelview_attrib: GLuint,

	pub eyepos_attrib: GLuint
}

impl Shader {
	pub async fn Default() -> Result<Self> {
		Self::load_files("pkg/graphics/shaders/flat.vertex.glsl",
						 "pkg/graphics/shaders/flat.fragment.glsl").await
	}

	pub async fn Gouraud() -> Result<Self> {
		Self::load_files("pkg/graphics/shaders/gouraud.vertex.glsl",
						 "pkg/graphics/shaders/gouraud.fragment.glsl").await
	}

	pub async fn Phong() -> Result<Self> {
		Self::load_files("pkg/graphics/shaders/phong.vertex.glsl",
						 "pkg/graphics/shaders/phong.fragment.glsl").await
	}

	pub async fn load_files(vertex_path: &str,
							fragment_path: &str) -> Result<Self> {
		let vertex_src = async_std::fs::read_to_string(vertex_path).await?;
		let fragment_src = async_std::fs::read_to_string(fragment_path).await?;
		Self::load(vertex_src.as_ref(), fragment_src.as_ref())
	}

	// TODO: Clean up on Drop (also if only part of it succeeds, we should clean
	// up just that part).
	pub fn load(vertex_src: &str, fragment_str: &str) -> Result<Self> {
		unsafe {
			let vertex_shader = gl_create_shader(gl::VERTEX_SHADER,
												 vertex_src)?;
			let fragment_shader = gl_create_shader(gl::FRAGMENT_SHADER,
												   fragment_str)?;

			let program = gl::CreateProgram();
			gl::AttachShader(program, vertex_shader);
			gl::AttachShader(program, fragment_shader);

			//glBindFragDataLocation(shaderProgram, 0, "outColor");

			gl::LinkProgram(program);

			let pos_attrib = gl_get_attrib(program, b"position\0");
			let normal_attrib = gl_get_attrib(program, b"normal\0");
			let color_attrib = gl_get_attrib(program, b"color\0");
			let uni_modelview_attrib = gl_get_attrib(program, b"modelview\0");
			let uni_proj_attrib = gl_get_attrib(program, b"proj\0");
			let eyepos_attrib = gl_get_attrib(program, b"eyePosition\0");

			Ok(Self {
				program,
				pos_attrib, normal_attrib, color_attrib, uni_proj_attrib,
				uni_modelview_attrib,
				eyepos_attrib
			})
		}
	}

	/// Initialize the shader for the current vertex array object
	pub fn init(&self) {
		unsafe {
			gl::EnableVertexAttribArray(self.pos_attrib);
			gl::EnableVertexAttribArray(self.normal_attrib);
			gl::EnableVertexAttribArray(self.color_attrib);
		}
	}

	pub fn set_lights(&self, lights: &[LightSource]) {
		if lights.len() > MAX_LIGHTS {
			panic!("Too many lights!");
		}

		let nlights_attr = gl_get_attrib(self.program, b"nlights\0");
		unsafe {
			gl::Uniform1i(nlights_attr as GLint, lights.len() as GLint);
		}

		for i in 0..lights.len() {
			let p_attr = gl_get_location(
				self.program, format!("lights[{}].position", i).as_bytes());
			let a_attr = gl_get_location(
				self.program, format!("lights[{}].ambient", i).as_bytes());
			let d_attr = gl_get_location(
				self.program, format!("lights[{}].diffuse", i).as_bytes());
			let s_attr = gl_get_location(
				self.program, format!("lights[{}].specular", i).as_bytes());

			gl_uniform_vec4(p_attr, &lights[i].position);
			gl_uniform_vec3(a_attr, &lights[i].ambient);
			gl_uniform_vec3(d_attr, &lights[i].diffuse);
			gl_uniform_vec3(s_attr, &lights[i].specular);
		}
	}

	pub fn set_material(&self, material: &Material) {
		let a_attr = gl_get_location(self.program, b"material.ambient\0");
		let d_attr = gl_get_location(self.program, b"material.diffuse\0");
		let s_attr = gl_get_location(self.program, b"material.specular\0");
		let sh_attr = gl_get_location(self.program, b"material.shininess\0");

		gl_uniform_vec3(a_attr, &material.ambient);
		gl_uniform_vec3(d_attr, &material.diffuse);
		gl_uniform_vec3(s_attr, &material.specular);
		unsafe {
			gl::Uniform1f(sh_attr as GLint, material.shininess);
		}
	}


}

fn gl_create_shader(typ: GLenum, src: &str) -> Result<GLuint> {
	let shader = unsafe { gl::CreateShader(typ) };
	// TODO: Check if this is correct for a utf-8 string
	let strings = [unsafe { std::mem::transmute(src.as_ptr()) }];
	let lengths = [src.len() as GLint];
	unsafe {
		gl::ShaderSource(shader, 1, strings.as_ptr(), lengths.as_ptr());
		gl::CompileShader(shader);
	}

	let mut status = 0;
	unsafe {
		gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);
	}

	if status != (gl::TRUE as i32) {
		let mut length = MAX_ERROR_LENGTH;
		let mut error_log = vec![];
		error_log.resize(length as usize, 0);
		unsafe {
			gl::GetShaderInfoLog(shader, length, &mut length, &mut error_log[0]);
		}

		// TODO: Does this include the null terminator.
		let length = length as usize;
		if length > error_log.len() {
			return Err("Invalid error length returned by OpenGL".into());
		}

		return Err(format!("Shader failed to compile: {}",
						  std::str::from_utf8(unsafe { std::mem::transmute(
							  &error_log[..length]) })?).into());
	}

	Ok(shader)
}

fn gl_get_attrib(program: GLuint, name: &[u8]) -> GLuint {
	assert!(name.len() > 0 && *name.last().unwrap() == 0);
	let attr = unsafe {
		gl::GetAttribLocation(
			program,
			std::mem::transmute::<*const u8, *const i8>(name.as_ptr()))
	};
	assert!(attr >= 0);
	attr as GLuint
}

fn gl_get_location(program: GLuint, name: &[u8]) -> GLuint {
	assert!(name.len() > 0 && *name.last().unwrap() == 0);
	let loc = unsafe {
		gl::GetUniformLocation(program, std::mem::transmute(name.as_ptr()))
	};
	assert!(loc >= 0);
	loc as GLuint
}
