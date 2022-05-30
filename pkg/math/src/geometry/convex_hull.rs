use alloc::vec::Vec;
use core::cmp::Ordering;

use common::errors::*;

use crate::matrix::{Vector2f, Vector3f};

/// Given a set of points, returns the points which represent the convex full of
/// that set. The returned points are ordered in clockwise order.
///
/// Runtime is 'O(n log n)' for 'n' points.
pub fn convex_hull(points: &[Vector2f]) -> Result<Vec<Vector2f>> {
    let mut sorted_points = points.iter().cloned().collect::<Vec<_>>();

    // Sort by x. Points with the same x are sorted by y.
    sorted_points.sort_by(|a, b| {
        if a.x() == b.x() {
            return a.y().partial_cmp(&b.y()).unwrap_or(Ordering::Equal);
        }

        a.x().partial_cmp(&b.x()).unwrap_or(Ordering::Equal)
    });

    // TODO: Ignore points that are almost exactly the same (within some rounding
    // error).

    if sorted_points.len() < 3 {
        return Err(err_msg("Can't compute convex hull of < 3 points"));
    }

    let mut upper_hull = vec![];
    upper_hull.push(sorted_points[0].clone());
    upper_hull.push(sorted_points[1].clone());

    for next_point in &sorted_points[2..] {
        upper_hull.push(next_point.clone());
        remove_ending_left_turns(&mut upper_hull);
    }

    let mut lower_hull = vec![];
    lower_hull.push(sorted_points[sorted_points.len() - 1].clone());
    lower_hull.push(sorted_points[sorted_points.len() - 2].clone());

    for next_point in sorted_points[..(sorted_points.len() - 2)].iter().rev() {
        lower_hull.push(next_point.clone());
        remove_ending_left_turns(&mut lower_hull);
    }

    // Return both upper and lower hull
    // Ignore the first and last points in the lower hull as they are duplicated of
    // the ones in the upper hull.
    upper_hull.extend_from_slice(&lower_hull[1..(lower_hull.len() - 1)]);
    Ok(upper_hull)
}

/// While the last 3 points in the hull don't turn right, remove the second to
/// last point.
fn remove_ending_left_turns(hull: &mut Vec<Vector2f>) {
    loop {
        if hull.len() < 3 {
            break;
        }

        let i = hull.len() - 3;
        let j = hull.len() - 2;
        let k = hull.len() - 1;
        if turns_right(&hull[i], &hull[j], &hull[k]) {
            break;
        }

        hull.remove(j);
    }
}

/// Returns true if we are only making a right (or straight) turn when
/// connecting the ray AB to BC.
pub fn turns_right(a: &Vector2f, b: &Vector2f, c: &Vector2f) -> bool {
    let ab = b - a;
    let ac = c - a;

    let ab3 = Vector3f::from_slice(&[ab.x(), ab.y(), 0.]);
    let ac3 = Vector3f::from_slice(&[ac.x(), ac.y(), 0.]);

    let n = ab3.cross(&ac3);

    n.z() <= 0.
}

fn vec2f(x: f32, y: f32) -> Vector2f {
    Vector2f::from_slice(&[x, y])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turns_right_test() {
        assert_eq!(
            turns_right(&vec2f(10., 10.), &vec2f(20., 20.), &vec2f(30., 10.)),
            true
        );

        assert_eq!(
            turns_right(&vec2f(10., 10.), &vec2f(20., 20.), &vec2f(30., 30.)),
            true
        );

        assert_eq!(
            turns_right(&vec2f(10., 10.), &vec2f(20., 20.), &vec2f(30., 40.)),
            false
        );

        assert_eq!(
            turns_right(&vec2f(10., 10.), &vec2f(20., 0.), &vec2f(30., 8.)),
            false
        );
    }

    #[test]
    fn convex_hull_test() {
        let data = vec![
            Vector2f::from_slice(&[335.00, 172.00]),
            Vector2f::from_slice(&[207.00, 260.00]),
            Vector2f::from_slice(&[221.00, 377.00]),
            Vector2f::from_slice(&[295.00, 505.00]),
            Vector2f::from_slice(&[502.00, 590.00]),
            Vector2f::from_slice(&[599.00, 482.00]),
            Vector2f::from_slice(&[596.00, 338.00]),
            Vector2f::from_slice(&[462.00, 263.00]),
            Vector2f::from_slice(&[511.00, 209.00]),
            Vector2f::from_slice(&[301.00, 272.00]),
            Vector2f::from_slice(&[410.00, 409.00]),
            Vector2f::from_slice(&[421.00, 516.00]),
            Vector2f::from_slice(&[540.00, 502.00]),
            Vector2f::from_slice(&[525.00, 396.00]),
            Vector2f::from_slice(&[309.00, 415.00]),
            Vector2f::from_slice(&[241.00, 313.00]),
            Vector2f::from_slice(&[391.00, 223.00]),
            Vector2f::from_slice(&[346.00, 342.00]),
            Vector2f::from_slice(&[497.00, 337.00]),
            Vector2f::from_slice(&[391.00, 286.00]),
            Vector2f::from_slice(&[361.00, 464.00]),
        ];

        let hull = convex_hull(&data).unwrap();

        let expected = vec![
            Vector2f::from_slice(&[207.00, 260.00]),
            Vector2f::from_slice(&[221.00, 377.00]),
            Vector2f::from_slice(&[295.00, 505.00]),
            Vector2f::from_slice(&[502.00, 590.00]),
            Vector2f::from_slice(&[599.00, 482.00]),
            Vector2f::from_slice(&[596.00, 338.00]),
            Vector2f::from_slice(&[511.00, 209.00]),
            Vector2f::from_slice(&[335.00, 172.00]),
        ];

        // TODO: Accept any rotation of these elements.
        assert_eq!(&hull, &expected);
    }
}
