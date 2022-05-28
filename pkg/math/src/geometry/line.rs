use alloc::vec::Vec;

use crate::matrix::{Matrix2f, Vector2f};

/// Representation of an unbounded 2d line where a point is defined as:
/// p = base + (lambda * dir)
pub struct Line2f {
    pub base: Vector2f,
    pub dir: Vector2f,
}

impl Line2f {
    /// NOTE: When using this formulation, it is guaranteed that base will equal
    /// x1 and (base + dir) will equal x2. So this can be used to recover the
    /// original line segment. But all other operations still assume that the
    /// line is continuous.
    pub fn from_points(x1: &Vector2f, x2: &Vector2f) -> Self {
        Self {
            base: (*x1).clone(),
            dir: x2 - x1,
        }
    }

    /// Given that self is: p = base1 + (lambda1 * dir1)
    /// and other is:       p = base2 + (lambda2 * dir2)
    ///
    /// Then the intersection is:
    ///   base1 + (lambda1 * dir1) = base2 + (lambda2 * dir2)
    ///   (lambda1 * dir1) - (lambda2 * dir2) = base2 - base1
    pub fn intersect(&self, other: &Line2f) -> Option<Vector2f> {
        let mut A = Matrix2f::zero();
        A.block_mut(0, 0).copy_from(&self.dir);
        A.block_mut(0, 1).copy_from(&other.dir);

        let b = &other.base - &self.base;

        if A.determinant().abs() < 1e-6 {
            None
        } else {
            let x = A.inverse() * b;
            Some(self.evaluate(x[0]))
        }
    }

    pub fn evaluate(&self, t: f32) -> Vector2f {
        &self.base + (self.dir.to_owned() * t)
    }
}

pub struct HalfEdgeDataStruct {
    faces: Vec<Face>,
    half_edges: Vec<HalfEdge>,
}

struct Face {
    outer_component: Option<usize>,
    inner_components: Vec<usize>,
}

struct HalfEdge {
    origin: Vector2f,
    twin: usize,
    incident_face: usize,
    next: usize,
    prev: usize,
}

/*
pub fn overlap_polys(segments: &[LineSegment2f]) {
    // Compute all intersection points

    // Dedup points and form edge list
    // - Need to lookup point in

    // Traverse edges clockwise to form polygons

    // Keep going until we have all half-edges.
    // - Don't need to make a polygon if we can't go clockwise.

    // Map back data from original faces?

    //
}
*/
