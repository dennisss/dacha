use alloc::rc::Rc;
use core::f32::consts::PI;
use core::ops::{Deref, DerefMut};
use std::sync::Mutex;

use gl::types::{GLint, GLuint};
use math::matrix::{vec2f, Vector2f, Vector3f};

use crate::opengl::drawable::Drawable;
use crate::opengl::object::Object;
use crate::opengl::shader::Shader;
use crate::opengl::texture::Texture;
use crate::opengl::window::Window;
use crate::transform::{Camera, Transform};

/// Convex polygon drawing
pub struct Polygon {
    object: Object,
    nvertices: usize,
}

impl_deref!(Polygon::object as Object);

impl Polygon {
    /// Creates a regular polygon centered at (0,0,0) with vertices sampled with
    /// the x-y unit circle.
    pub fn regular(nsides: usize, shader: Rc<Shader>) -> Self {
        let vertices = regular_polygon(nsides);
        Self::from(&vertices, shader)
    }

    pub fn regular_mono(nsides: usize, shader: Rc<Shader>) -> Self {
        Self::regular(nsides, shader)
    }

    pub fn rectangle(top_left: Vector2f, width: f32, height: f32, shader: Rc<Shader>) -> Self {
        let mut vertices = vec![];

        let z = 1.;

        vertices.push(Vector3f::from_slice(&[top_left.x(), top_left.y(), z]));
        vertices.push(Vector3f::from_slice(&[
            top_left.x() + width,
            top_left.y(),
            z,
        ]));
        vertices.push(Vector3f::from_slice(&[
            top_left.x() + width,
            top_left.y() + height,
            z,
        ]));
        vertices.push(Vector3f::from_slice(&[
            top_left.x(),
            top_left.y() + height,
            z,
        ]));

        let mut obj = Self::from(&vertices, shader);
        obj.set_vertex_texture_coordinates(&[
            vec2f(0., 0.),
            vec2f(1., 0.),
            vec2f(1., 1.),
            vec2f(0., 1.),
        ]);

        obj
    }

    pub fn from(vertices: &[Vector3f], shader: Rc<Shader>) -> Self {
        let mut object = Object::new(shader.clone());

        object.set_vertex_positions(vertices);

        Self {
            object,
            nvertices: vertices.len(),
        }
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

    let dtheta = 2.0 * PI / (nsides as f32);
    let mut theta = -(PI / 2.0) + (dtheta / 2.0);

    for i in 0..nsides {
        pts.push(Vector3f::from_slice(&[theta.cos(), theta.sin(), 0.0]));
        theta += dtheta;
    }

    pts
}
