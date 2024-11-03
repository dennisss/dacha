use crate::matrix::{Matrix3f, Vector2f, Vector3f};

/// Applies a transformation matrix to a 2d point.
pub fn transform2f(mat: &Matrix3f, p: &Vector2f) -> Vector2f {
    let p3 = mat * Vector3f::from((p.clone(), 1.));
    Vector2f::from_slice(&[p3.x(), p3.y()])
}
