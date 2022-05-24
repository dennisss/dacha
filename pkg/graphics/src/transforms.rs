use math::matrix::{Matrix3f, Matrix4f, Vector2f, Vector3f};

#[inline]
pub fn translate(v: &Vector3f) -> Matrix4f {
    Matrix4f::from_slice(&[
        1.,
        0.,
        0.,
        v.x(),
        0.,
        1.,
        0.,
        v.y(),
        0.,
        0.,
        1.,
        v.z(),
        0.,
        0.,
        0.,
        1.,
    ])
}

#[inline]
pub fn scale(v: &Vector3f) -> Matrix4f {
    Matrix4f::from_slice(&[
        v.x(),
        0.,
        0.,
        0.,
        0.,
        v.y(),
        0.,
        0.,
        0.,
        0.,
        v.z(),
        0.,
        0.,
        0.,
        0.,
        1.,
    ])
}

#[inline]
pub fn translate2f(v: Vector2f) -> Matrix3f {
    Matrix3f::from_slice(&[1., 0., v.x(), 0., 1., v.y(), 0., 0., 1.])
}

#[inline]
pub fn scale2f(v: &Vector2f) -> Matrix3f {
    Matrix3f::from_slice(&[v.x(), 0., 0., 0., v.y(), 0., 0., 0., 1.0])
}

/*
inline mat4 rotate(GLfloat t, vec3 u){
    return mat4(
        cos(t) + u.x*u.x*(1.0 - cos(t)), u.x*u.y*(1.0 - cos(t)) - u.z*sin(t), u.x*u.z*(1.0 - cos(t)) + u.y*sin(t), 0,
        u.y*u.x*(1.0-cos(t)) + u.z*sin(t), cos(t) + u.y*u.y*(1.0 - cos(t)), u.y*u.z*(1.0 - cos(t)) - u.x*sin(t), 0,
        u.z*u.x*(1.0-cos(t)) - u.y*sin(t), u.z*u.y*(1.0 - cos(t)) + u.x*sin(t), cos(t) + u.z*u.z*(1.0 - cos(t)), 0,
        0, 0, 0, 1
    );
}
*/
