use math::matrix::{Vector2f, Vector3f};

use gl::types::GLuint;

use crate::opengl::util::*;

/// Values attached to each attribute in a shader for rendering a
/// single entity.
///
/// - There is a single field in this struct for every possible
///   ShaderAttributeId value.
/// - Because most shaders will only use a subset of these attribute ids, only
///   some of these will contain a non-Empty value.
#[derive(Default)]
pub(super) struct ShaderAttributeValues {
    pub positions: ShaderAttributeValue<Vector3f>,
    pub colors: ShaderAttributeValue<Vector3f>,
    pub normals: ShaderAttributeValue<Vector3f>,
    pub texture_coordinates: ShaderAttributeValue<Vector2f>,
    pub alphas: ShaderAttributeValue<f32>,
}

/// Values associated with a single per-vertex attribute.
///
/// - 'T' is the type used for representing the value for a single vertex.
/// - This object contains information on which value of 'T' should be used for
///   each vertex in a rendered entity.
pub enum ShaderAttributeValue<T: ShaderAttributeType> {
    /// No values have been associated with this attribute yet.
    /// If it is an error for a value to be Empty if the shader expects the
    /// respective attribute as an input.
    Empty,

    /// The values of this attribute for each vertex are stored in the given
    /// buffer (this buffer has already been bound to a VAO).
    Buffer(GLBuffer),

    /// Each vertex should get the same given value for this attribute.
    Value(T),
}

impl<T: ShaderAttributeType> Default for ShaderAttributeValue<T> {
    fn default() -> Self {
        Self::Empty
    }
}

impl<T: ShaderAttributeType> ShaderAttributeValue<T> {
    /// Assuming a VAO for an entity is currently bound, this
    pub(super) unsafe fn assign<V: ToShaderAttributeValue<T>>(
        &mut self,
        index: GLuint,
        new_value: V,
    ) {
        /// If a buffer is already
        if let Self::Buffer(_) = self {
            // TODO: Make sure this is the same as the index used to generate it.
            gl::DisableVertexAttribArray(index);
        }

        *self = new_value.to_value(index);

        if let Self::Buffer(_) = self {
            gl::EnableVertexAttribArray(index);
        }
    }

    pub(super) unsafe fn bind(&self, index: GLuint) -> bool {
        match self {
            ShaderAttributeValue::Empty => false,
            ShaderAttributeValue::Buffer(_) => true,
            ShaderAttributeValue::Value(value) => {
                T::bind_value(index, value);
                true
            }
        }
    }
}

pub trait ShaderAttributeType {
    unsafe fn bind_value(index: GLuint, value: &Self);
    unsafe fn bind_values(index: GLuint, values: &[Self]) -> GLBuffer
    where
        Self: Sized;
}

impl ShaderAttributeType for f32 {
    unsafe fn bind_value(index: GLuint, value: &Self) {
        gl::VertexAttrib1f(index, *value);
    }

    unsafe fn bind_values(index: GLuint, values: &[Self]) -> GLBuffer {
        todo!()
    }
}

impl ShaderAttributeType for Vector2f {
    unsafe fn bind_value(index: GLuint, value: &Self) {
        gl::VertexAttrib2f(index, value[0], value[1])
    }

    unsafe fn bind_values(index: GLuint, values: &[Self]) -> GLBuffer {
        gl_vertex_buffer_vec2(index, values)
    }
}

impl ShaderAttributeType for Vector3f {
    unsafe fn bind_value(index: GLuint, value: &Self) {
        gl::VertexAttrib3f(index, value[0], value[1], value[2])
    }

    unsafe fn bind_values(index: GLuint, values: &[Self]) -> GLBuffer {
        gl_vertex_buffer_vec3(index, values)
    }
}

/// Value that can be used to create a ShaderAttributeValue.
///
/// This is basically a wrapper around calling:
/// - ShaderAttributeType::bind_value for scalar types
/// - ShaderAttributeType::bind_values for slice types.
pub trait ToShaderAttributeValue<T: ShaderAttributeType> {
    unsafe fn to_value(self, index: GLuint) -> ShaderAttributeValue<T>;
}

impl<T: ShaderAttributeType> ToShaderAttributeValue<T> for T {
    unsafe fn to_value(self, index: GLuint) -> ShaderAttributeValue<T> {
        ShaderAttributeValue::Value(self)
    }
}

impl<T: ShaderAttributeType> ToShaderAttributeValue<T> for &[T] {
    unsafe fn to_value(self, index: GLuint) -> ShaderAttributeValue<T> {
        ShaderAttributeValue::Buffer(T::bind_values(index, self))
    }
}

impl<T: ShaderAttributeType, const LEN: usize> ToShaderAttributeValue<T> for &[T; LEN] {
    unsafe fn to_value(self, index: GLuint) -> ShaderAttributeValue<T> {
        ShaderAttributeValue::Buffer(T::bind_values(index, &self[..]))
    }
}
