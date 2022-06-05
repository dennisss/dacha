use math::matrix::{vec2f, Matrix3f, Vector2f};

use crate::canvas::curve::Curve;
use crate::transforms::transform2f;

#[derive(Debug)]
pub struct Ellipse {
    pub center: Vector2f,

    pub x_axis: Vector2f,
    pub y_axis: Vector2f,

    /// Angle at t=0 in radians.
    pub start_angle: f32,

    /// Change in angle relative to start_angle at t=1.
    ///
    /// TODO: Our linearization strategy assumes that only the positions at t=0
    /// and t=1 can be equal, so we need to ensure that the magnitude of this is
    /// always <= 2*pi.
    pub delta_angle: f32,
}

impl Curve for Ellipse {
    fn transform(&self, mat: &Matrix3f) -> Self {
        let center = transform2f(mat, &self.center);

        Self {
            center: center.clone(),
            x_axis: transform2f(mat, &(&self.x_axis + &self.center)) - &center,
            y_axis: transform2f(mat, &(&self.y_axis + &self.center)) - &center,
            start_angle: self.start_angle,
            delta_angle: self.delta_angle,
        }
    }

    fn evaluate(&self, t: f32) -> Vector2f {
        let angle = self.start_angle + (t * self.delta_angle);
        &self.center + (self.x_axis.clone() * angle.cos()) + (self.y_axis.clone() * angle.sin())
    }

    fn linearize(&self, max_error: f32, output: &mut Vec<Vector2f>) {
        // Assuming the delta angle is <= 2*PI, the midpoint bisection method used by
        // this should always accurately estimate the max error.
        crate::canvas::linearize::linearize_midpoint_bisection(self, max_error, output)
    }
}
