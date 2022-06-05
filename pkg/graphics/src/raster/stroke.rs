use common::iter::{PairIter, PairIterator};
use image::Color;
use math::geometry::line::Line2f;
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
pub fn stroke_poly(points: &[Vector2f], width: f32) -> Vec<Vector2f> {
    let mut out = vec![];

    let mut offset_segments = |iter: PairIterator<Vector2f>| {
        let mut last_line = None;

        for (p_i, p_j) in iter {
            let mut l = Line2f::from_points(p_i, p_j);
            if l.dir.norm() < 1e-6 {
                // Skip empty lines.
                continue;
            }

            // Normal vector to the line.
            let n: Vector2f = {
                let a = Vector3f::from_slice(&[l.dir.x(), l.dir.y(), 0.0]);
                let b = Vector3f::from_slice(&[0.0, 0.0, 1.0]);
                let c = a.cross(&b);
                c.block(0, 0).to_owned().normalized()
            };

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

            last_line = Some(l);
        }

        // For the last line, we just take the offset endpoint of the original segment
        // as the stroke point.
        if let Some(line) = last_line.take() {
            out.push(line.base + line.dir);
        }
    };

    offset_segments(points.pair_iter());
    offset_segments(points.pair_iter().rev());

    out
}
