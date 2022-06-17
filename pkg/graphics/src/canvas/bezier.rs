// See http://agg.sourceforge.net/antigrain.com/research/adaptive_bezier/index.html for an interseting dicussion on properly linearizing beziers.

use core::convert::{AsMut, AsRef};

use math::combin::bin_coeff;
use math::geometry::line_segment::LineSegment2;
use math::matrix::{Matrix3f, Vector2f, Vector3f};

use crate::canvas::curve::Curve;
use crate::transforms::transform2f;

use super::linearize::LINEARIZATION_MIN_STEP;

pub trait Points = Clone + AsRef<[Vector2f]> + AsMut<[Vector2f]>;

#[derive(Debug)]
pub struct BezierCurve<P = Vec<Vector2f>> {
    pub points: P,
}

impl<P: Points> BezierCurve<P> {
    /// Measures the flatness of the bezier curve by summing the L1 distance of
    /// the curve's bounding box to the center line.
    fn flatness(&self) -> f32 {
        let points = self.points.as_ref();

        let center_line = &points[points.len() - 1] - &points[0];

        let distance = center_line.norm();
        let dir: Vector3f = (center_line.normalized(), 0.).into();

        // Error tangential to the center line.
        let mut pos_cross: f32 = 0.;
        let mut neg_cross: f32 = 0.;

        // Error parallel to the center line.
        let mut pos_dot: f32 = 0.;
        let mut neg_dot: f32 = 0.;

        for p in &points[1..(points.len() - 1)] {
            let p_vec: Vector3f = (p - &points[0], 0.).into();
            // TODO: Consider separately maintaining max negative and positives and summing
            // them later.
            let c = dir.cross(&p_vec).z();
            if c >= 0. {
                pos_cross = pos_cross.max(c);
            } else {
                neg_cross = neg_cross.min(c);
            }

            let d = dir.dot(&p_vec);
            if d >= 0. {
                pos_dot = pos_dot.max(d);
            } else {
                neg_dot = neg_dot.min(d);
            }
        }

        // We only care how much the control points go beyond the end of the center
        // line.
        pos_dot = (pos_dot - distance).max(0.);

        (pos_dot - neg_dot) + (pos_cross - neg_cross)
    }

    /// NOTE: start_t and end_t are the positions in the original curve.
    fn linearize_inner(
        &self,
        max_error: f32,
        start_t: f32,
        end_t: f32,
        output: &mut Vec<Vector2f>,
    ) {
        let points = self.points.as_ref();

        if end_t - start_t <= LINEARIZATION_MIN_STEP || self.flatness() <= max_error {
            output.push(points[points.len() - 1].clone());
            return;
        }

        // Use De Casteljau's algorithm to cut the curve in half.

        let t = 0.5;

        // Control points for the first half of the curve.
        let mut beta_0 = self.points.clone();

        // Control points for the second half of the curve.
        let mut beta = self.points.clone();
        for j in 1..points.len() {
            for i in 0..(points.len() - j) {
                beta.as_mut()[i] =
                    beta.as_ref()[i].clone() * (1. - t) + beta.as_ref()[i + 1].clone() * t;
            }

            beta_0.as_mut()[j] = beta.as_ref()[0].clone();
        }

        let mid_t = (end_t + start_t) / 2.;

        Self { points: beta_0 }.linearize_inner(max_error, start_t, mid_t, output);
        Self { points: beta }.linearize_inner(max_error, mid_t, end_t, output);
    }
}

impl<P: Points> Curve for BezierCurve<P> {
    fn transform(&self, mat: &Matrix3f) -> Self {
        let mut points = self.points.clone();
        for i in 0..points.as_ref().len() {
            let v = transform2f(mat, &points.as_ref()[i]);
            points.as_mut()[i] = v;
        }

        Self { points }
    }

    fn evaluate(&self, t: f32) -> Vector2f {
        let points = self.points.as_ref();

        if t == 0.0 {
            return points[0].clone();
        } else if t == 1.0 {
            return points.last().cloned().unwrap();
        }

        let mut sum = Vector2f::zero();
        let n = points.len() - 1;
        for i in 0..points.len() {
            let coeff =
                (bin_coeff(n, i) as f32) * (1.0 - t).powi((n - i) as i32) * t.powi(i as i32);
            sum += points[i].clone() * coeff;
        }

        sum
    }

    fn linearize(&self, max_error: f32, output: &mut Vec<Vector2f>) {
        let points = self.points.as_ref();

        output.push(points[0].clone());
        self.linearize_inner(max_error, 0., 1., output);
    }
}
