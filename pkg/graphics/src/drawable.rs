use std::sync::{Arc, Mutex, Weak};

use gl::types::{GLint, GLuint};
use math::matrix::{Matrix4f, Vector3f};

use crate::lighting::Material;
use crate::shader::Shader;
use crate::transform::{AsMatrix, Camera, Transform};
use crate::util::*;
use crate::window::Window;

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

/// Every object has its own vertex array object
/// TODO: Inherits Drawable
pub struct Object {
    primitive: Primitive,

    // protected:
    shader: Arc<Shader>,
    material: Option<Arc<Material>>,

    // private:
    vao: GLuint,
}

impl_deref!(Object::primitive as Primitive);

impl Object {
    pub fn new(shader: Arc<Shader>) -> Self {
        let mut vao = 0;
        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
        }

        shader.init();

        Self {
            primitive: Primitive::default(),
            shader,
            material: None,
            vao,
        }
    }

    pub fn shader(&self) -> &Shader {
        self.shader.as_ref()
    }

    // Works for shaders when the current shader shares the exact same
    // attributes (and ordering of them) as the new shader
    pub fn set_shader(&mut self, shader: Arc<Shader>) {
        self.shader = shader;
    }

    pub fn set_material(&mut self, material: Arc<Material>) {
        self.material = Some(material);
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}

impl Drawable for Object {
    fn draw(&self, cam: &Camera, model_view: &Transform) {
        unsafe {
            gl::BindVertexArray(self.vao);
            gl::UseProgram(self.shader.program);
        }

        let p = cam.matrix();
        gl_uniform_mat4(self.shader.uni_proj_attrib, &p);

        self.shader.set_lights(&cam.lights);

        let mv = model_view.matrix() * self.primitive.transform();

        gl_uniform_mat4(self.shader.uni_modelview_attrib, &mv);

        if let Some(attr) = self.shader.eyepos_attrib {
            gl_uniform_vec3(attr, &cam.position);
        }

        if let Some(material) = &self.material {
            self.shader.set_material(material);
        }
    }
}
