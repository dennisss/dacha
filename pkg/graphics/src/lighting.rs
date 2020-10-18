use math::matrix::{Vector3f, Vector4f};

// These should match the shaders

pub const MAX_LIGHTS: usize = 4;

pub struct LightSource {
    pub position: Vector4f,

    pub ambient: Vector3f,
    pub diffuse: Vector3f,
    pub specular: Vector3f,
}

pub struct Material {
    pub ambient: Vector3f,
    pub diffuse: Vector3f,
    pub specular: Vector3f,
    pub shininess: f32,
}
