use gl::types::{GLuint, GLint};
use math::matrix::Vector3f;
use std::sync::Arc;
use crate::shader::Shader;
use crate::drawable::{Object, Drawable};
use std::ops::{DerefMut, Deref};
use crate::util::{gl_vertex_buffer_vec3, GLBuffer};
use crate::transform::{Camera, Transform};

pub const PI: f32 = 3.14159265359;

/// Convex polygon drawing
pub struct Polygon {
	object: Object,

	pos_vbo: GLBuffer,
	color_vbo: GLBuffer,
	nvertices: usize
}

impl_deref!(Polygon::object as Object);

impl Polygon {
	pub fn regular(nsides: usize, colors: &[Vector3f], shader: Arc<Shader>)
		-> Self {
		assert_eq!(nsides, colors.len());
		let vertices = regular_polygon(nsides);
		Self::from(&vertices, &colors, shader)
	}

	pub fn regular_mono(
		nsides: usize, color: &Vector3f, shader: Arc<Shader>) -> Self {

		let mut colors: Vec<Vector3f> = vec![];
		colors.resize(nsides, color.clone());

		Self::regular(nsides, &colors, shader)
	}

	pub fn from(vertices: &[Vector3f], colors: &[Vector3f], shader: Arc<Shader>)
		-> Self {
		assert_eq!(vertices.len(), colors.len());


		let object = Object::new(shader.clone()); // <- Will bind the VAO
		let pos_vbo = gl_vertex_buffer_vec3(shader.pos_attrib, vertices);
		let color_vbo = gl_vertex_buffer_vec3(shader.color_attrib, colors);

		Self {
			object,
			pos_vbo, color_vbo, nvertices: vertices.len()
		}
	}

	// Changes the color of all vertices to one color
	pub fn set_color(&self, color: &Vector3f) {
//		vector<vec3> colors(this->nvertices, color);
//		glBindBuffer(GL_ARRAY_BUFFER, color_vbo);
//		glBufferData(GL_ARRAY_BUFFER, sizeof(vec3) * colors.size(), &colors[0], GL_STATIC_DRAW);
	}

}

impl Drawable for Polygon {
	fn draw(&self, camera: &Camera, prev: &Transform) {
		self.object.draw(camera, prev);
		unsafe {
			gl::DrawArrays(gl::TRIANGLE_FAN, 0, self.nvertices as GLint);
		}
	}
}


fn regular_polygon(nsides: usize) -> Vec<Vector3f> {
	let mut pts = vec![];

	let dtheta = 2.0*PI / (nsides as f32);
	let mut theta = -(PI/2.0) + (dtheta / 2.0);

	for i in 0..nsides {
		pts.push(Vector3f::from_slice(&[ theta.cos(), theta.sin(), 0.0 ]));
		theta += dtheta;
	}

	pts
}
