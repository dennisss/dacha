use std::rc::Rc;
use std::sync::{Arc, Mutex, Weak};

use gl::types::{GLint, GLuint};
use math::matrix::{Matrix4f, Vector3f};

use crate::lighting::Material;
use crate::opengl::shader::Shader;
use crate::opengl::util::*;
use crate::opengl::window::Window;
use crate::transform::{AsMatrix, Camera, Transform};

/// An object than can be drawn. This class handles configuring transforms,
/// viewports, and projection
pub trait Drawable {
    /// Draw this object. 'proj' contains all transformations that should be
    /// applied to it
    fn draw(&self, cam: &Camera, model_view: &Transform);
}

pub struct Primitive {
    transform: Matrix4f,
}

impl Default for Primitive {
    fn default() -> Self {
        Self {
            transform: Matrix4f::identity(),
        }
    }
}

impl Primitive {
    pub fn transform(&self) -> &Matrix4f {
        &self.transform
    }

    pub fn set_transform(&mut self, matrix: Matrix4f) {
        self.transform = matrix;
    }

    pub fn apply(&mut self, matrix: &Matrix4f) {
        self.transform = &self.transform * matrix;
    }
}
