use std::collections::VecDeque;

use math::matrix::Vector2f;

use crate::canvas::curve::Curve;

// /// When converting a curve into a set of line segments, the line size at
// which /// we will stop subdividing the curve.
// /// TODO: Instead threshold based on change in slope.
// const LINEARIZATION_ERROR_THRESHOLD: f32 = 2.0;

/// Limit on the increment of t we will use when linearizing a curve. This
/// corresponds to a limit of 500 segments.
pub const LINEARIZATION_MIN_STEP: f32 = 0.002;

/// Converts an arbitrary curve into a set of line segments which approximate
/// it.
///
/// - We start be initially trying to approximate the curve as one segment at
///   t=0 and t=1.
/// - Then we test if splitting the line segment into 2 at the midpoint
///   significantly deviates from the original line segment.
/// - If it does, we recursively try splitting each half of the segment until we
///   have a good enough result.
///
/// So if the curve is sufficiently non-linear that it periodically repeats the
/// same point, then this won't work very well.
pub fn linearize_midpoint_bisection<C: Curve>(
    curve: &C,
    max_error: f32,
    output: &mut Vec<Vector2f>,
) {
    let max_error_squared = max_error * max_error;

    let mut current_t = 0.;
    let mut current_point = curve.evaluate(current_t);
    output.push(current_point.clone());

    // Next points to consider as the endpoint as the line segment starting at
    // current_point in order of ascending 't' value.
    let mut next_points = VecDeque::new();
    next_points.push_back((1., curve.evaluate(1.)));

    while let Some((next_t, next_point)) = next_points.get(0).cloned() {
        let mid_t = (next_t + current_t) / 2.;
        let mid_point_expected = curve.evaluate(mid_t);

        let mid_point_actual = (&next_point + &current_point) / 2.;

        if next_t - current_t <= LINEARIZATION_MIN_STEP
            || (mid_point_actual - &mid_point_expected).norm_squared() <= max_error_squared
        {
            output.push(next_point.clone());

            current_t = next_t;
            current_point = next_point;
            next_points.pop_front();
        } else {
            next_points.push_front((mid_t, mid_point_expected));
        }
    }
}
