use core::f32::consts::PI;
use core::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};

use gl::types::{GLint, GLuint};
use math::matrix::{Vector2f, Vector3f};

use crate::opengl::drawable::{Drawable, Object};
use crate::opengl::shader::Shader;
use crate::opengl::texture::Texture;
use crate::opengl::util::{gl_vertex_buffer_vec2, gl_vertex_buffer_vec3, GLBuffer};
use crate::opengl::window::Window;
use crate::transform::{Camera, Transform};

/// Convex polygon drawing
pub struct Polygon {
    object: Object,

    pos_vbo: GLBuffer,
    color_vbo: Option<GLBuffer>,

    texture: Option<Arc<Texture>>,
    texture_coordinates_vbo: Option<GLBuffer>,

    nvertices: usize,
}

impl_deref!(Polygon::object as Object);

impl Polygon {
    /// Creates a regular polygon centered at (0,0,0) with vertices sampled with
    /// the x-y unit circle.
    pub fn regular(nsides: usize, colors: &[Vector3f], shader: Arc<Shader>) -> Self {
        assert_eq!(nsides, colors.len());
        let vertices = regular_polygon(nsides);
        Self::from(&vertices, &colors, shader)
    }

    pub fn regular_mono(nsides: usize, color: &Vector3f, shader: Arc<Shader>) -> Self {
        let mut colors: Vec<Vector3f> = vec![];
        colors.resize(nsides, color.clone());

        Self::regular(nsides, &colors, shader)
    }

    pub fn rectangle(
        top_left: Vector2f,
        width: f32,
        height: f32,
        color: Vector3f,
        shader: Arc<Shader>,
    ) -> Self {
        let mut vertices = vec![];

        vertices.push(Vector3f::from_slice(&[top_left.x(), top_left.y(), 0.0]));
        vertices.push(Vector3f::from_slice(&[
            top_left.x() + width,
            top_left.y(),
            0.0,
        ]));
        vertices.push(Vector3f::from_slice(&[
            top_left.x() + width,
            top_left.y() + height,
            0.0,
        ]));
        vertices.push(Vector3f::from_slice(&[
            top_left.x(),
            top_left.y() + height,
            0.0,
        ]));

        let mut colors: Vec<Vector3f> = vec![];
        colors.resize(4, color.clone());

        Self::from(&vertices, &colors, shader)
    }

    pub fn from(vertices: &[Vector3f], colors: &[Vector3f], shader: Arc<Shader>) -> Self {
        assert_eq!(vertices.len(), colors.len());

        let object = Object::new(shader.clone()); // <- Will bind the VAO
        let pos_vbo = gl_vertex_buffer_vec3(shader.pos_attrib, vertices);
        let color_vbo = shader
            .color_attrib
            .map(|attr| gl_vertex_buffer_vec3(attr, colors));

        Self {
            object,
            pos_vbo,
            color_vbo,
            texture: None,
            texture_coordinates_vbo: None,
            nvertices: vertices.len(),
        }
    }

    /// MUST be called immediately after from().
    pub fn set_texture(&mut self, texture: Arc<Texture>, tex_coords: &[Vector2f]) {
        self.texture = Some(texture);
        self.texture_coordinates_vbo = Some(gl_vertex_buffer_vec2(
            self.shader().tex_coord_attrib.unwrap(),
            tex_coords,
        ));
    }

    // Changes the color of all vertices to one color
    pub fn set_color(&self, color: &Vector3f) {
        //		vector<vec3> colors(this->nvertices, color);
        //		glBindBuffer(GL_ARRAY_BUFFER, color_vbo);
        //		glBufferData(GL_ARRAY_BUFFER, sizeof(vec3) * colors.size(),
        // &colors[0], GL_STATIC_DRAW);
    }
}

impl Drawable for Polygon {
    fn draw(&self, camera: &Camera, prev: &Transform) {
        self.object.draw(camera, prev);

        if let Some(texture) = &self.texture {
            texture.bind();
        }

        unsafe {
            gl::DrawArrays(gl::TRIANGLE_FAN, 0, self.nvertices as GLint);
        }
    }
}

fn regular_polygon(nsides: usize) -> Vec<Vector3f> {
    let mut pts = vec![];

    let dtheta = 2.0 * PI / (nsides as f32);
    let mut theta = -(PI / 2.0) + (dtheta / 2.0);

    for i in 0..nsides {
        pts.push(Vector3f::from_slice(&[theta.cos(), theta.sin(), 0.0]));
        theta += dtheta;
    }

    pts
}
