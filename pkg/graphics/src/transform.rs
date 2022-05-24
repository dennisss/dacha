use math::matrix::{Matrix4f, Vector3f};

use crate::lighting::LightSource;

pub trait AsMatrix {
    fn matrix(&self) -> &Matrix4f;
}

pub struct Transform {
    transform: Matrix4f,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            transform: Matrix4f::identity(),
        }
    }
}

impl AsMatrix for Transform {
    fn matrix(&self) -> &Matrix4f {
        &self.transform
    }
}

impl Transform {
    pub fn from(transform: Matrix4f) -> Self {
        Self { transform }
    }

    pub fn apply(&self, rhs: &Matrix4f) -> Self {
        Self::from(&self.transform * rhs)
    }
}

pub struct Camera {
    pub lights: Vec<LightSource>,

    pub view: Matrix4f,
    pub proj: Matrix4f,

    // Same purpose as in transform.
    pub transform: Matrix4f,

    pub position: Vector3f,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            lights: vec![],
            view: Matrix4f::identity(),
            proj: Matrix4f::identity(),
            transform: Matrix4f::identity(),
            position: Vector3f::zero(),
        }
    }
}

impl Camera {
    pub fn matrix(&self) -> Matrix4f {
        &self.proj * &self.transform
    }
}

pub fn orthogonal_projection(
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    z_near: f32,
    z_far: f32,
) -> Matrix4f {
    Matrix4f::from_slice(&[
        2.0 / (right - left),
        0.0,
        0.0,
        -(right + left) / (right - left), //
        0.0,
        2.0 / (top - bottom),
        0.0,
        -(top + bottom) / (top - bottom), //
        0.0,
        0.0,
        -2.0 / (z_far - z_near),
        -(z_far + z_near) / (z_far - z_near), //
        0.0,
        0.0,
        0.0,
        1.0,
    ])
}

//impl AsMatrix for Camera {

//}

/*
// Compute a transformation that normalizes (centers and scales to -1 to 1) the points
glm::mat4 Normalizing(std::vector<glm::vec3> &pts);

glm::mat4 Normalizing(std::vector<glm::vec3> &pts){

    float xmin = FLT_MAX, xmax = FLT_MIN,
          ymin = FLT_MAX, ymax = FLT_MIN,
          zmin = FLT_MAX, zmax = FLT_MIN;

    // Compute AABB
    for(const glm::vec3 &v : pts){
        if(v.x < xmin)
            xmin = v.x;
        else if(v.x > xmax)
            xmax = v.x;

        if(v.y < ymin)
            ymin = v.y;
        else if(v.y > ymax)
            ymax = v.y;

        if(v.z < zmin)
            zmin = v.z;
        else if(v.z > zmax)
            zmax = v.z;
    }


    return glm::scale(glm::vec3(
        2.0f / (xmax - xmin),
        2.0f / (ymax - ymin),
        2.0f / (zmax - zmin)
    )) * glm::translate(glm::vec3(
        -(xmax + xmin) / 2.0f,
        -(ymax + ymin) / 2.0f,
        -(zmax + zmin) / 2.0f
    ));
}

*/
