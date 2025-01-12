use std::time::Instant;

use alloc::vec::Vec;

use crate::matrix::{vec2f, Vector2f};

use super::{
    convex_hull::turns_right,
    curve::Curve2,
    ellipse::Ellipse,
    half_edge::{FaceDebug, FaceLabel, HalfEdgeStruct},
};

/// Offsets all faces in the given struct by inflating/deflating each boundary
/// in the parallel direction by a fixed distance.
///
/// - 'initial_faces': Initial set of faces before offsetting.
///   - All faces with a non-empty/default label will be offset.
/// - 'offset' amount to offset line segments in the perpendicular direction.
///   - >0 will make a outer polygon
///   - <0 will make an inner polygon.
/// - 'max_error' is the max allowed linearization error when creating line
///   segment arcs.
///
/// Returns a new half edge struct with the faces offset. Faces that still have
/// a non-empty/default label represent the offsets. Note that face ids will not
/// correspond to those in the original faces but labels will be preserved as
/// best as possible.
pub fn offset_faces<F: FaceLabel + PartialEq>(
    initial_faces: &HalfEdgeStruct<F>,
    offset: f32,
    max_error: f32,
) -> HalfEdgeStruct<F> {
    if offset.abs() <= 0.001 || offset.abs() <= max_error {
        return initial_faces.clone();
    }

    let mut out = initial_faces.map_labels(|f| FaceOffsetingLabel {
        base_label: f.clone(),
        is_offset: false,
    });

    for face in initial_faces.faces() {
        if face.label() == &F::default() {
            continue;
        }

        let offset_label = FaceOffsetingLabel {
            base_label: face.label().clone(),
            is_offset: true,
        };

        if let Some(component) = face.outer_component() {
            offset_face_boundary(
                offset_label.clone(),
                &component.points(),
                offset,
                max_error,
                &mut out,
            );
        }

        for component in face.inner_components() {
            offset_face_boundary(
                offset_label.clone(),
                &component.points(),
                offset,
                max_error,
                &mut out,
            );
        }
    }

    out.repair();

    let mut out = out.map_labels(|l| {
        // When deflating, we want to delete any parts of the original faces that is in
        // the offset part. For inflation, we just want to union everything.
        if offset < 0.0 && l.is_offset {
            return F::default();
        }

        l.base_label.clone()
    });

    out
}

#[derive(Clone, Default, Debug, PartialEq)]
struct FaceOffsetingLabel<F> {
    base_label: F,

    /// If true, the face was generated as an extension of a pre-existing base
    /// edge. False implies that it is one of the original faces before
    /// offsetting started.
    is_offset: bool,
}

impl<F: FaceLabel> FaceLabel for FaceOffsetingLabel<F> {
    fn union(&self, other: &Self) -> Self {
        Self {
            base_label: self.base_label.union(&other.base_label),
            is_offset: self.is_offset || other.is_offset,
        }
    }
}

// NOTE: The given points will be from a HalfEdgeStruct so will be in
// couterclockwise order around a face.
fn offset_face_boundary<F: FaceLabel>(
    label: F,
    points: &[Vector2f],
    offset: f32,
    max_error: f32,
    out: &mut HalfEdgeStruct<F>,
) {
    let inflating = offset > 0.0;

    for i in 0..points.len() {
        // 'Current' segment being offset is 'p1 -> p2'
        let p1 = &points[i];
        let p2 = &points[(i + 1) % points.len()];
        // The 'next' segment to be offset is 'p2 -> p3'
        let p3 = &points[(i + 2) % points.len()];

        let dir = (p2 - p1).normalized();
        // Assuming segments are ordered counterclockwise, this will point out of the
        // face (right of the edge).
        let perp = Vector2f::from_slice(&[dir.y(), -dir.x()]);

        let delta = perp * offset;

        let q1 = p1 + &delta;
        let q2 = p2 + &delta;

        // Rectangle extending out from the original edge.
        // (points in counter-clockwise order)
        let rect_points = {
            if inflating {
                [p1.clone(), q1, q2.clone(), p2.clone()]
            } else {
                // Reverse order of the other case.
                [p1.clone(), p2.clone(), q2.clone(), q1]
            }
        };
        out.add_face(label.clone(), rect_points.iter().cloned());

        let turns_left = !turns_right(p1, p2, p3);

        if turns_left == inflating {
            // In this case, we are a concave angle (if inflating) so the offset rectangles
            // won't intersect. We need to join the rectangles with a arc segment.
            // - Center is at 'p2'
            // - Radius is 'offset'
            // - Must pass through 'q2' ('p2' shifted based on 'current' segment)
            // - Must pass through 'q2_2' ('p2' shifted based on the 'next' segment)
            // - There are two possible arcs with this definition though we always want the
            //   one on the same side as the direction of our offsetting.

            let dir2 = (p3 - p2).normalized();
            let perp2 = Vector2f::from_slice(&[dir2.y(), -dir2.x()]);
            let delta2 = perp2 * offset;

            let q2_2 = p2 + delta2;

            // TODO: Verify that both points at not after also being quantized
            // in the half edge

            add_arc_segment(label.clone(), &p2, &q2, &q2_2, max_error, inflating, out);
        }
    }

    //
}

fn add_arc_segment<'a, F: FaceLabel>(
    label: F,
    center: &Vector2f,
    mut p1: &'a Vector2f,
    mut p2: &'a Vector2f,
    max_error: f32,
    inflating: bool,
    out: &mut HalfEdgeStruct<F>,
) {
    let dir1 = p1 - center;
    let angle1 = dir1.y().atan2(dir1.x());

    let dir2 = p2 - center;
    let mut angle2 = dir2.y().atan2(dir2.x());

    let radius = dir1.norm();

    let (start_angle, mut end_angle) = {
        if inflating {
            // 'angle1' is the smaller angle and we need to increase in angle until we get
            // to 'angle2'
            (angle1, angle2)
        } else {
            core::mem::swap::<&Vector2f>(&mut p1, &mut p2);

            // 'angle2' is the smaller angle and we need to increase in angle until we get
            // to 'angle1'
            (angle2, angle1)
        }
    };

    if start_angle > end_angle {
        end_angle += 2.0 * std::f32::consts::PI;
    }

    let ellipse = Ellipse {
        center: center.clone(),
        x_axis: vec2f(radius, 0.0),
        y_axis: vec2f(0.0, radius),
        start_angle,
        delta_angle: (end_angle - start_angle),
    };

    let mut points = vec![];
    ellipse.linearize(max_error, &mut points);

    // Ensure that the start and end point of the arc are exactly equal to the
    // original points.
    points[0] = p1.clone();
    *points.last_mut().unwrap() = p2.clone();

    points.push(center.clone());

    out.add_face(label, points.into_iter());
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    use testing::*;

    fn label(s: &'static str) -> HashSet<&'static str> {
        let mut l = HashSet::new();
        l.insert(s);
        l
    }

    fn labels(s: &[&'static str]) -> HashSet<&'static str> {
        let mut l = HashSet::new();
        for s in s {
            l.insert(*s);
        }
        l
    }

    #[test]
    fn offset_merging() {
        let mut half_edges = HalfEdgeStruct::<HashSet<&'static str>>::new();

        half_edges.add_face(
            label("A"),
            [
                vec2f(1.0, 1.0),
                vec2f(4.0, 1.0),
                vec2f(4.0, 4.0),
                vec2f(1.0, 4.0),
            ]
            .iter()
            .cloned(),
        );

        // Left
        half_edges.add_face(
            label("B"),
            [
                vec2f(1.0, 1.0),
                vec2f(2.0, 1.0),
                vec2f(2.0, 4.0),
                vec2f(1.0, 4.0),
            ]
            .iter()
            .cloned(),
        );

        // Right
        half_edges.add_face(
            label("C"),
            [
                vec2f(3.0, 1.0),
                vec2f(4.0, 1.0),
                vec2f(4.0, 4.0),
                vec2f(3.0, 4.0),
            ]
            .iter()
            .cloned(),
        );

        // Bottom
        half_edges.add_face(
            label("D"),
            [
                vec2f(1.0, 1.0),
                vec2f(4.0, 1.0),
                vec2f(4.0, 2.0),
                vec2f(1.0, 2.0),
            ]
            .iter()
            .cloned(),
        );

        // Top
        half_edges.add_face(
            label("E"),
            [
                vec2f(1.0, 3.0),
                vec2f(4.0, 3.0),
                vec2f(4.0, 4.0),
                vec2f(1.0, 4.0),
            ]
            .iter()
            .cloned(),
        );

        half_edges.repair();

        /*
        Middle space with just the 'A' label:
        - (2,2), (3,2), (3,3), (2,3)
        */

        let faces = FaceDebug::get_all(&half_edges);

        println!("{:?}", faces);

        let mut found = false;
        for face in faces {
            if face.label == labels(&["A"]) {
                found = true;
                break;
            }
        }

        assert!(found);
    }

    #[test]
    fn deflating_test() {
        let mut half_edges = HalfEdgeStruct::<bool>::new_with_scale(1000.0);

        half_edges.add_face(
            true,
            [
                vec2f(0.0, 0.0),
                vec2f(1.0, 0.0),
                vec2f(1.0, 1.0),
                vec2f(0.0, 1.0),
            ]
            .iter()
            .cloned(),
        );

        let mut offset = offset_faces(&half_edges, -0.1, 0.01);

        offset.merge_faces();

        let faces = FaceDebug::get_all(&offset);

        assert_that(
            &faces,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: false,
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.1, 0.1),
                        vec2f(0.1, 0.9),
                        vec2f(0.9, 0.9),
                        vec2f(0.9, 0.1),
                    ]],
                }),
                eq(FaceDebug {
                    label: true,
                    outer_component: Some(vec![
                        vec2f(0.1, 0.1),
                        vec2f(0.9, 0.1),
                        vec2f(0.9, 0.9),
                        vec2f(0.1, 0.9),
                    ]),
                    inner_components: vec![],
                }),
            ]),
        );
    }
}
