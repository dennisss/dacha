use alloc::rc::Rc;
use std::collections::HashMap;
use std::sync::{Mutex, Weak};

use gl::types::{GLint, GLuint};
use math::matrix::{Matrix4f, Vector2f, Vector3f};

use crate::lighting::Material;
use crate::opengl::drawable::Drawable;
use crate::opengl::shader::*;
use crate::opengl::shader_attributes::*;
use crate::opengl::texture::Texture;
use crate::opengl::util::*;
use crate::opengl::window::Window;
use crate::opengl::window::WindowContext;
use crate::transform::{AsMatrix, Camera, Transform};

// TODO: If we ever support adopting another buffer, we must support verifying
// that it is from the vertex opengl context/window.

// TODO: Ensure that every single attribute and uniform in the shader is
// assigned some value by this object.

// TODO: Validate that all vertex attributes are the same length.

/// Every object has its own vertex array object
/// TODO: Inherits Drawable
pub struct Object {
    window_context: WindowContext,
    vao: GLuint,
    shader: Rc<Shader>,
    attr_values: ShaderAttributeValues,
    texture: Option<Rc<Texture>>,
    material: Option<Rc<Material>>,
}

macro_rules! set_attribute {
    ($f:ident, $t:ty, $prop:ident, $id:ident) => {
        pub fn $f<T: ToShaderAttributeValue<$t>>(&mut self, value: T) -> &mut Self {
            let index = match self.shader.attrs.get(&ShaderAttributeId::$id) {
                Some(index) => *index,
                // TODO: Consider making this return an error.
                None => return self,
            };

            self.bind();
            unsafe { self.attr_values.$prop.assign(index, value) };
            self
        }
    };
}

impl Object {
    pub fn new(mut window_context: WindowContext, shader: Rc<Shader>) -> Self {
        window_context.make_current();

        let mut vao = 0;
        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
        }

        Self {
            window_context,
            vao,
            shader,
            attr_values: ShaderAttributeValues::default(),
            texture: None,
            material: None,
        }
    }

    fn bind(&mut self) {
        self.window_context.make_current();
        unsafe { gl::BindVertexArray(self.vao) };
    }

    pub fn shader(&self) -> &Shader {
        self.shader.as_ref()
    }

    set_attribute!(set_vertex_positions, Vector3f, positions, VertexPosition);
    set_attribute!(set_vertex_colors, Vector3f, colors, VertexColor);
    set_attribute!(set_vertex_normals, Vector3f, normals, VertexNormal);
    set_attribute!(
        set_vertex_texture_coordinates,
        Vector2f,
        texture_coordinates,
        VertexTextureCoordinate
    );
    set_attribute!(set_vertex_alphas, f32, alphas, VertexAlpha);

    // pub fn set_element_indices(&mut self, indices: &[[GLuint; 3]]) -> &mut Self {
    //     // gl_face_buffer
    // }

    pub fn set_texture(&mut self, texture: Rc<Texture>) -> &mut Self {
        // TODO: Texture must be from the same window context as us.

        // TODO: Verify that all the vertex buffers are of the same length.
        self.texture = Some(texture);
        self
    }

    pub fn set_material(&mut self, material: Rc<Material>) {
        self.material = Some(material);
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        self.window_context.make_current();
        unsafe {
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}

impl Drawable for Object {
    fn draw(&self, cam: &Camera, model_view: &Transform) {
        // TODO: Must also ensure that the window context is current.
        unsafe {
            gl::BindVertexArray(self.vao);
            gl::UseProgram(self.shader.program);
        }

        // TODO: Sort the list.
        let mut missing_ids = vec![];

        // NOTE: Constant attribute values (not backed by a buffer) are not stored in
        // the VAO so we need to bind their values here.
        for (id, index) in self.shader.attrs.iter() {
            let has_value = unsafe {
                match id {
                    ShaderAttributeId::VertexPosition => self.attr_values.positions.bind(*index),
                    ShaderAttributeId::VertexColor => self.attr_values.colors.bind(*index),
                    ShaderAttributeId::VertexAlpha => self.attr_values.alphas.bind(*index),
                    ShaderAttributeId::VertexTextureCoordinate => {
                        self.attr_values.texture_coordinates.bind(*index)
                    }
                    ShaderAttributeId::VertexNormal => self.attr_values.normals.bind(*index),
                }
            };

            if !has_value {
                missing_ids.push(*id);
            }
        }

        if !missing_ids.is_empty() {
            panic!("Some vertex attributes missing values: {:?}", missing_ids);
        }

        // TODO: Loop over all uniforms and make sure we set them all to something!

        // TODO: Can we do this once at a higher level to avoid setting it every time.
        let p = cam.matrix();
        gl_uniform_mat4(
            self.shader
                .uniforms
                .get(&ShaderUniformId::ProjectionMatrix)
                .cloned()
                .unwrap(),
            &p,
        );

        self.shader.set_lights(&cam.lights);

        gl_uniform_mat4(
            self.shader
                .uniforms
                .get(&ShaderUniformId::ModelViewMatrix)
                .cloned()
                .unwrap(),
            model_view.matrix(),
        );

        if let Some(attr) = self
            .shader
            .uniforms
            .get(&ShaderUniformId::CameraPosition)
            .cloned()
        {
            gl_uniform_vec3(attr, &cam.position);
        }

        if let Some(material) = &self.material {
            self.shader.set_material(material);
        }

        if let Some(texture) = &self.texture {
            texture.bind();
        }
    }
}
