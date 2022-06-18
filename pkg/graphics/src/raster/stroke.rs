use common::iter::{PairIter, PairIterator};
use image::Color;
use math::geometry::line::Line2;
use math::matrix::{Matrix2f, Vector2f, Vector3f};

use crate::raster::PolygonRef;

// Any edge shared by two triangles can be trivially discarded from future
// testing.

// Pick remaining

/// Converts a possibly complex polygon into a set of non-overlapping trapezoids
/// (or triangles).
fn trapezoidalize(poly: PolygonRef) {
    // Vec<Vector2f, >
    // A line segment on the scan line and exactly two lines

    // Sort values by y value

    // Find intersections

    // Create line segments

    // Match old line segments to new ones to create trapezoids.
}

pub struct StrokeStyle {
    color: Color,
    line_width: f32,
    /// If true, we will join the last line point with the first.
    closed: bool,
    /// Units per stroked and empty segments. If empty, the entire path is
    /// stroked.
    dash_array: Vec<f32>,
}

// TODO: Must transform the units of the dash_array based on the view port.

// TODO: Needs to support 'closed'
pub fn stroke_split_dashes(points: &[Vector2f], dash_array: &[f32]) -> Vec<Vec<Vector2f>> {
    // TODO: Optimize the case of an empty dash array

    if points.is_empty() {
        return vec![];
    }
    if dash_array.is_empty() {
        return vec![points.to_vec()];
    }

    let mut dash_i = 0;
    let mut dash_len = 0.0;

    let mut dashes = vec![];
    let mut cur = vec![points[0].clone()]; // TODO: Check not empty.

    let mut j = 1;
    while j < points.len() {
        let p_i = cur.last().unwrap();
        let p_j = &points[j];
        let dir = p_j - p_i;
        let len = dir.norm();

        let remaining_len = dash_array[dash_i] - dash_len;

        if remaining_len >= len {
            cur.push((*p_j).clone());
            dash_len += len;
            j += 1;
        } else {
            let pt = p_i + dir.normalized() * remaining_len;
            cur.push(pt.clone());

            if dash_i % 2 == 0 {
                dashes.push(cur.split_off(0));
            } else {
                cur.clear();
            }
            cur.push(pt);

            dash_i = (dash_i + 1) % dash_array.len();
            dash_len = 0.0;
        }
    }

    dashes
}

/// Given points which form one continuous sub path, this will generate a stroke
/// polygon that wraps the path.
///
/// This is implements by offsetting the lines by width/2. It will create the
/// polygon by first offseting in one direction in the forward direction and
/// then in the reverse direction. To ensure that offset segments intersect, the
/// line intersections are recomputed after offsetting.
///
/// TODO: Need a special case for closed paths where interpolate the last
/// point based on intersection with the first line.
pub fn stroke_poly(points: &[Vector2f], width: f32) -> (Vec<Vector2f>, Vec<usize>) {
    let mut out = vec![];
    let mut path_starts = vec![];

    let mut closed = points.len() > 0 && points[0] == points[points.len() - 1];

    path_starts.push(0);

    let mut offset_segments = |iter: PairIterator<Vector2f>, out: &mut Vec<Vector2f>| {
        let mut start_index = out.len();

        let mut first_line = None;
        let mut last_line = None;

        for (p_i, p_j) in iter {
            let mut l = Line2::from_points(p_i, p_j);
            if l.dir.norm() < 1e-6 {
                // Skip empty lines.
                continue;
            }

            // Normal vector to the line.
            let n: Vector2f = l.perp().normalized();

            let offset = n * (width / 2.0);

            l.base += &offset;

            if let Some(line) = &last_line {
                if let Some(inter) = l.intersect(line) {
                    out.push(inter);
                } else {
                    // Otherwise, the lines are parallel, so they have a trivial intersection
                    // point.
                    out.push(l.base.clone());
                }
            } else {
                out.push(l.base.clone());
            }

            if first_line.is_none() {
                first_line = Some(l.clone());
            }
            last_line = Some(l);
        }

        if closed && first_line.is_some() && last_line.is_some() {
            let first = first_line.unwrap();
            let last = last_line.unwrap();

            if let Some(inter) = first.intersect(&last) {
                out[start_index] = inter;
            }

            out.push(out[start_index].clone());
        } else if let Some(line) = last_line.take() {
            // For the last line, we just take the offset endpoint of the original segment
            // as the stroke point.
            out.push(line.base + line.dir);
        }
    };

    offset_segments(points.pair_iter(), &mut out);

    if closed {
        path_starts.push(out.len());
    }

    offset_segments(points.pair_iter().rev(), &mut out);

    path_starts.push(out.len());

    (out, path_starts)
}

#[cfg(test)]
mod tests {
    use super::*;

    use math::matrix::vec2f;

    #[test]
    fn single_line() {
        let points = &[
            vec2f(0., 0.),
            vec2f(10., 0.),
            vec2f(10., 10.),
            vec2f(0., 10.),
            vec2f(0., 0.),
        ];

        println!("{:#?}", stroke_poly(points, 2.));
    }
}
