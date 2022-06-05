use math::matrix::{Matrix3f, Vector2f};

/// A continous function over 2d points.
pub trait Curve {
    fn transform(&self, mat: &Matrix3f) -> Self;

    fn evaluate(&self, t: f32) -> Vector2f;

    /// Convert the curve to a set of points which when connected with line
    /// segments in order approximate the curve.
    fn linearize(&self, max_error: f32, output: &mut Vec<Vector2f>);
}
