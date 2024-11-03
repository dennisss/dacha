use alloc::vec::Vec;

use crate::matrix::{Matrix3f, Vector2f};

/// A continous function over 2d points.
pub trait Curve2 {
    fn transform(&self, mat: &Matrix3f) -> Self;

    /// Evaluates the curve at some fraction of the way to the end.
    /// 't' is defined from 0 to 1.
    fn evaluate(&self, t: f32) -> Vector2f;

    /// Convert the curve to a set of points which when connected with line
    /// segments in order approximate the curve.
    fn linearize(&self, max_error: f32, output: &mut Vec<Vector2f>);
}
