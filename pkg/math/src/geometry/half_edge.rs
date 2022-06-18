use alloc::vec::Vec;
use core::cmp::Ordering;
use core::f32::consts::PI;
use core::fmt::Debug;
use core::hash::Hash;
use std::collections::HashMap;
use std::collections::HashSet;

use common::loops::*;

use crate::geometry::convex_hull::turns_right;
use crate::geometry::entity_storage::*;
use crate::geometry::line_segment::{
    compare_points, compare_points_i64, compare_points_x_then_y, LineSegment2,
};
use crate::geometry::quantized::*;
use crate::matrix::Vector2i64;
use crate::matrix::{vec2f, Vector2f};
use crate::rational::Rational;

/*
Design details:
- Uses quantized vectors internally. (1000x the size of the float points)
- But intersections are computed in floating point
- In floating point, we use a THRESHOLD to compare points, but in the quantized space, we compare points exactly.
    - The assumption here is that THRESHOLD << 1000 so we can assume that two quantized points that are off by one are not equal.

Why this needs to use quantization?
- Speed
- When intersections as removed, we want to ensure that splitting a line doesn't make it an empty size line
- When we check for overlapping segments, we want a consistent calculation at both endpoints of the overlap.


The face associated with each edge lies to the left of the ddge.

Half edges stored in counterclockwise order

Hole boundaries have edges sorted in clockwise order.

TODOs:
- Need resilience to having multiple edges which use duplicate start/end points.

*/

pub trait FaceLabel: Clone + Default + Debug {
    // TODO: Maybe use BitOr instead?
    fn union(&self, other: &Self) -> Self;
}

impl FaceLabel for () {
    fn union(&self, other: &Self) -> Self {
        ()
    }
}

impl<T: Clone + Debug + Hash + PartialEq + Eq> FaceLabel for HashSet<T> {
    fn union(&self, other: &Self) -> Self {
        self | other
    }
}

#[derive(Debug)]
pub struct HalfEdgeStruct<F> {
    half_edges: EntityStorage<EdgeTag, HalfEdge>,
    faces: EntityStorage<FaceTag, Face<F>>,
    unbounded_face_id: FaceId,
}

#[derive(Debug, Clone)]
struct Face<Label> {
    label: Label,

    /// Some edge on the outer most boundary of this face.
    /// If none, then this is the unbounded face surrounding all polygons.
    outer_component: Option<EdgeId>,

    /// Some edge of each face inside the outer component (holes).
    inner_components: Vec<EdgeId>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BoundaryType {
    Inner,
    Outer,
}

#[derive(Clone, Debug)]
struct HalfEdge {
    origin: Vector2i64,
    twin: EdgeId,

    incident_face: FaceId,
    next: EdgeId,
    prev: EdgeId,
}

impl<F: FaceLabel> HalfEdgeStruct<F> {
    pub fn new() -> Self {
        let half_edges = EntityStorage::new();

        let mut faces = EntityStorage::new();

        let unbounded_face_id = faces.unique_id();

        faces.insert(
            unbounded_face_id,
            Face {
                label: F::default(),
                outer_component: None,
                inner_components: vec![],
            },
        );

        Self {
            half_edges,
            faces,
            unbounded_face_id,
        }
    }

    // NOTE: Label will be the inner face if the polygon is built with
    // counter-clockwise vertices.
    pub fn add_first_edge(&mut self, start: Vector2f, end: Vector2f, label: F) -> EdgeId {
        let id = self.half_edges.unique_id();
        let twin = self.half_edges.unique_id();
        let face_id = self.faces.unique_id();

        self.faces.insert(
            face_id,
            Face {
                label,
                outer_component: Some(id),
                inner_components: vec![],
            },
        );

        // TODO: I need to check if the vertices are going clockwise or
        // counter-clockwise after the polygon is constructed to tell if we are
        // assigning the right face to the right edge.
        self.half_edges.insert(
            id,
            HalfEdge {
                origin: quantize2(start),
                twin,
                incident_face: face_id,
                next: twin,
                prev: twin,
            },
        );
        self.half_edges.insert(
            twin,
            HalfEdge {
                origin: quantize2(end),
                twin: id,
                incident_face: self.unbounded_face_id,
                next: id,
                prev: id,
            },
        );

        id
    }

    // Helper for adding a line to a chain
    pub fn add_next_edge(&mut self, prev: EdgeId, next_point: Vector2f) -> EdgeId {
        let id = self.half_edges.unique_id();
        let twin = self.half_edges.unique_id();

        let prev_twin = self.half_edges[prev].twin;
        let last_point = self.destination(&self.half_edges[prev]);

        let incident_face = self.half_edges[prev].incident_face;

        self.half_edges.insert(
            id,
            HalfEdge {
                origin: last_point,
                twin,
                incident_face,
                next: twin,
                prev,
            },
        );
        self.half_edges[prev].next = id;

        self.half_edges.insert(
            twin,
            HalfEdge {
                origin: quantize2(next_point),
                twin: id,
                incident_face: self.unbounded_face_id,
                next: prev_twin,
                prev: id,
            },
        );
        self.half_edges[prev_twin].prev = twin;

        id
    }

    pub fn add_close_edge(&mut self, last_edge: EdgeId, first_edge: EdgeId) {
        let id = self.half_edges.unique_id();
        let twin = self.half_edges.unique_id();

        let last_origin = self.half_edges[last_edge].origin.clone();
        let last_dest = self.destination(&self.half_edges[last_edge]);
        let last_twin = self.half_edges[last_edge].twin;

        let first_origin = self.half_edges[first_edge].origin.clone();
        let first_twin = self.half_edges[first_edge].twin;

        let incident_face = self.half_edges[last_edge].incident_face;

        self.half_edges.insert(
            id,
            HalfEdge {
                origin: last_dest,
                twin: twin,
                incident_face,
                next: first_edge,
                prev: last_edge,
            },
        );
        self.half_edges[last_edge].next = id;
        self.half_edges[first_edge].prev = id;

        self.half_edges.insert(
            twin,
            HalfEdge {
                origin: first_origin,
                twin: id,
                incident_face: self.unbounded_face_id,
                next: last_twin,
                prev: first_twin,
            },
        );
        self.half_edges[first_twin].next = twin;
        self.half_edges[last_twin].prev = twin;
    }

    fn destination(&self, edge: &HalfEdge) -> Vector2i64 {
        self.half_edges[edge.twin].origin.clone()
    }

    // TODO: How to deal with overlapping line segments (overlapping segments should
    // intersect that their ).
    pub fn overlap(&self, other: &Self) -> Self {
        // First concatenate the edge sets.
        // Ids of the second set at shifted to avoid overlaps.
        let mut output = {
            let mut half_edges = self.half_edges.clone();
            let edge_id_offset = half_edges.next_id;
            half_edges.next_id = half_edges.next_id + other.half_edges.next_id;

            let mut faces = self.faces.clone();
            let face_id_offset = faces.next_id;
            faces.next_id = faces.next_id + other.faces.next_id;

            // TODO: Merge the other's unbounded face components into this one.
            let unbounded_face_id = self.unbounded_face_id;

            for (id, edge) in other.half_edges.iter() {
                half_edges.insert(
                    *id + edge_id_offset,
                    HalfEdge {
                        origin: edge.origin.clone(),
                        incident_face: edge.incident_face + face_id_offset,
                        twin: edge.twin + edge_id_offset,
                        next: edge.next + edge_id_offset,
                        prev: edge.prev + edge_id_offset,
                    },
                );
            }
            for (id, face) in other.faces.iter() {
                faces.insert(
                    *id + face_id_offset,
                    Face {
                        label: face.label.clone(),
                        outer_component: face
                            .outer_component
                            .clone()
                            .map(|edge_id| edge_id + edge_id_offset),
                        inner_components: face
                            .inner_components
                            .iter()
                            .cloned()
                            .map(|edge_id| edge_id + edge_id_offset)
                            .collect(),
                    },
                );
            }

            Self {
                half_edges,
                faces,
                unbounded_face_id,
            }
        };

        output.repair();
        output
    }

    /// Makes the current edge/face set completely 'valid'. In particular, we
    /// want the structure to contain no intersecting/overlapping half edges or
    /// faces.
    pub fn repair(&mut self) {
        // Eliminate any edges with zero length.
        {
            let mut skip_ids = vec![];
            for (id, half_edge) in self.half_edges.iter() {
                if half_edge.origin == self.destination(half_edge) {
                    skip_ids.push(*id);
                }
            }

            for id in skip_ids {
                let edge = self.half_edges.remove(&id).unwrap();

                if let Some(prev) = self.half_edges.get_mut(&edge.prev) {
                    prev.next = edge.next;
                }

                if let Some(next) = self.half_edges.get_mut(&edge.next) {
                    next.prev = edge.prev;
                }
            }
        }

        let mut segments = vec![];

        // For each segment in 'segments' this is the id of the edge from which it was
        // derived.
        let mut segment_edge_ids = vec![];

        {
            for (id, half_edge) in self.half_edges.iter() {
                // Only index one half-edge per edge as they correct to the same line segment.
                if *id > half_edge.twin {
                    continue;
                }

                segments.push(LineSegment2 {
                    start: dequantize2::<f64>(half_edge.origin.clone()),
                    end: dequantize2::<f64>(self.destination(half_edge)),
                });
                segment_edge_ids.push(*id);
            }
        }

        // Id of the edge immediately to the left of the origin vertex of each left (if
        // any).
        let mut edge_left_neighbors = HashMap::new();

        // Id of each half edge which we are intending on deleting because it
        // overlaps with another edge.
        //
        // Note that deletions are deferred until after all intersections are
        // handled as each deleted edge should participate in 2 intersections
        // (the first marks the outward edge for deletion and the second marks
        // the opposite direction twin for deletion). The second twin edge may
        // not be immediately known as future intersections may split the edges
        // in half.
        let mut deleted_outward_ids = HashSet::new();

        let intersections = LineSegment2::intersections(&segments, 1e-6);

        for intersection in intersections {
            // TODO: Stop early if the intersection point is strictly on endpoints of
            // existing edges.

            // TODO: This MUST be an exact opposite operation to the
            let intersection_point = quantize2(intersection.point.clone());

            // println!("I {:?}", intersection_point);

            // Record of a pair of half-edges (twins) with one endpoint at the intersection
            // point and another somewhere else.
            #[derive(Debug)]
            struct PartialEdge {
                // Id of the half-edge directed towards the intersection point.
                inward_id: EdgeId,

                // Id of the edge immediately before the inward_id edge in the original graph.
                // NOTE: The original value of 'inward_next' will be another edge in
                // 'intersecting_edges' and the value of the next pointer will be recalculated
                // later.
                inward_prev: EdgeId,

                inward_face: FaceId,

                // Id of the edge directed away from the intersection point.
                outward_id: EdgeId,

                outward_next: EdgeId,

                outward_face: FaceId,

                // Other endpoint of this edge aside of the intersection.point.
                point: Vector2i64,

                angle: Rational,
                length: i64,
            }

            // List of all edges converging at the intersection point.
            let mut intersecting_edges = vec![];

            for segment_idx in intersection.segments.iter().cloned() {
                let edge_id = segment_edge_ids[segment_idx];
                let edge = self.half_edges[edge_id].clone();
                let edge_dest = self.destination(&edge);

                {
                    let segment = LineSegment2 {
                        start: dequantize2(edge.origin.clone()),
                        end: dequantize2(edge_dest.clone()),
                    };

                    assert!(segment.contains(&intersection.point, 1e-3));
                }

                // TODO: If our threshold is larger than one quantized unit, this must use
                // in-exact comparison.
                let origin_equal = edge.origin == intersection_point;
                let dest_equal = edge_dest == intersection_point;

                if origin_equal {
                    assert!(!dest_equal);

                    // if deleted_outward_ids.contains(&edge.twin) {
                    //     continue;
                    // }

                    // The current edge is outward.
                    // self.half_edges[edge.twin].next MUST also be in the current intersection.
                    intersecting_edges.push(PartialEdge {
                        inward_id: edge.twin,
                        inward_prev: self.half_edges[edge.twin].prev,
                        inward_face: self.half_edges[edge.twin].incident_face,
                        outward_id: edge_id,
                        outward_next: edge.next,
                        outward_face: edge.incident_face,
                        point: edge_dest,
                        angle: 0.into(), // Computed later.
                        length: 0,       // Computed later
                    });
                } else if dest_equal {
                    assert!(!origin_equal);

                    // if deleted_outward_ids.contains(&edge_id) {
                    //     continue;
                    // }

                    // The current edge is inward (opposite of first case).
                    // edge.next MUST also be in the current intersection as well.
                    intersecting_edges.push(PartialEdge {
                        inward_id: edge_id,
                        inward_prev: edge.prev,
                        inward_face: edge.incident_face,
                        outward_id: edge.twin,
                        outward_next: self.half_edges[edge.twin].next,
                        outward_face: self.half_edges[edge.twin].incident_face,
                        point: edge.origin.clone(),
                        angle: 0.into(), // Computed later.
                        length: 0,       // Computed later
                    });
                } else {
                    let id1 = self.half_edges.unique_id();
                    let id2 = self.half_edges.unique_id();

                    let mut e1 = PartialEdge {
                        inward_id: edge_id,
                        inward_prev: edge.prev,
                        inward_face: edge.incident_face,
                        outward_id: id1,
                        outward_next: self.half_edges[edge.twin].next,
                        outward_face: self.half_edges[edge.twin].incident_face,
                        point: edge.origin.clone(),
                        angle: 0.into(), // Computed later.
                        length: 0,       // Computed later
                    };

                    let mut e2 = PartialEdge {
                        inward_id: edge.twin,
                        inward_prev: self.half_edges[edge.twin].prev,
                        inward_face: self.half_edges[edge.twin].incident_face,
                        outward_id: id2,
                        outward_next: edge.next,
                        outward_face: edge.incident_face,
                        point: edge_dest.clone(),
                        angle: 0.into(), // Computed later.
                        length: 0,       // Computed later
                    };

                    // Compensation in the case that the line wraps around itself.
                    if e1.inward_prev == edge.twin {
                        e1.inward_prev = id1;
                    }
                    if e2.inward_prev == edge_id {
                        e2.inward_prev = id2;
                    }

                    // println!("{:?} <-> {:?}", e1.inward_id, e1.outward_id);
                    // println!("{:?} <-> {:?}", e2.inward_id, e2.outward_id);

                    // Update the segment to correct to the portion of the original segment which
                    // still remains to be matched below (/ to the right of) the sweep line.
                    segment_edge_ids[segment_idx] =
                        if compare_points_i64(&edge.origin, &edge_dest).is_gt() {
                            edge_id
                        } else {
                            edge.twin
                        };

                    // Make the split of the line durable.
                    // This ensures both sets of half edges are decoupled in case we want to delete
                    // one pair of them. Deleted edges can still appear when considering future
                    // intersections, so they must actually exist in the half_edges map.
                    for e in [&e1, &e2] {
                        self.half_edges.get_mut(&e.inward_id).unwrap().twin = e.outward_id;
                        self.half_edges.insert(
                            e.outward_id,
                            HalfEdge {
                                origin: intersection_point.clone(),
                                twin: e.inward_id,
                                incident_face: e.outward_face,
                                next: e.outward_next,
                                prev: Id::zero(), // Set later.
                            },
                        );
                    }

                    // if !deleted_outward_ids.contains(&e1.inward_id) {
                    //     self.half_edges.get_mut(&e1.outward_next).unwrap().prev = id1;
                    intersecting_edges.push(e1);
                    // }

                    // if !deleted_outward_ids.contains(&e2.inward_id) {
                    //     self.half_edges.get_mut(&e2.outward_next).unwrap().prev = id2;
                    intersecting_edges.push(e2);
                    // }
                }
            }

            // Sort edges by ascending clockwise angle
            for edge in &mut intersecting_edges {
                let dir = &edge.point - &intersection_point;
                // edge.angle = 2. * PI - dir.y().atan2(dir.x());
                edge.angle = dir.pseudo_angle();
                edge.length = dir.x().abs() + dir.y().abs();
            }
            intersecting_edges.sort_by(|a, b| {
                let angle_ordering = b.angle.cmp(&a.angle);
                if angle_ordering.is_ne() {
                    return angle_ordering;
                }

                // Edges which were previously deleted should be deleted again (sorted later in
                // the list).
                let a_deletion_mark = deleted_outward_ids.contains(&a.inward_id) as usize;
                let b_deletion_mark = deleted_outward_ids.contains(&b.inward_id) as usize;
                if a_deletion_mark != b_deletion_mark {
                    return a_deletion_mark.cmp(&b_deletion_mark);
                }

                // Descending sort by length.
                // We always want to delete the longer edge as we know that the retained one is
                // a subset of it.
                let len_ordering = a.length.cmp(&b.length);
                if len_ordering.is_ne() {
                    return len_ordering;
                }

                let a_id = core::cmp::min(a.inward_id, a.outward_id);
                let b_id = core::cmp::min(b.inward_id, b.outward_id);
                a_id.cmp(&b_id)
            });

            // Remove overlapping edges.
            intersecting_edges.dedup_by(|next_edge, edge| {
                if edge.angle == next_edge.angle {
                    // println!(
                    //     "Delete {:?} @ {}",
                    //     next_edge.outward_id,
                    //     next_edge.angle.to_f32()
                    // );

                    // NOTE: To avoid deleting an edge twice, we only insert one of its ids.
                    if !deleted_outward_ids.contains(&next_edge.inward_id) {
                        // println!("^ PAIR");
                        deleted_outward_ids.insert(next_edge.outward_id);
                    }

                    true
                } else {
                    false
                }
            });

            for (i, edge) in intersecting_edges.iter().enumerate() {
                let last_edge = &intersecting_edges[if i > 0 {
                    i - 1
                } else {
                    intersecting_edges.len() - 1
                }];
                let next_edge = &intersecting_edges[(i + 1) % intersecting_edges.len()];

                // Connect this inward edge to the next outward edge in clockwise order.
                self.half_edges.get_mut(&edge.inward_id).unwrap().next = next_edge.outward_id;
                self.half_edges.get_mut(&edge.outward_id).unwrap().prev = last_edge.inward_id;

                // println!("{:?} => {:?}", edge.inward_id, next_edge.outward_id);

                // Only needed if not done in the edge splitting code.
                self.half_edges.get_mut(&edge.outward_next).unwrap().prev = edge.outward_id;

                if let Some(left_neighbor) = intersection.left_neighbor.clone() {
                    edge_left_neighbors.insert(edge.outward_id, segment_edge_ids[left_neighbor]);
                }
            }
        }

        for id in deleted_outward_ids {
            let edge = self.half_edges.remove(&id).unwrap();
            self.half_edges.remove(&edge.twin);
        }

        #[derive(Debug)]
        struct Boundary {
            edges: Vec<EdgeId>,
            is_inner: bool,
            leftmost_vertex: EdgeId,

            self_faces: HashSet<FaceId>,

            // vertices: Vec<Vector2f>,
            parent: Option<usize>,

            // Indices of other boundaries which are children of this boundary.
            children: Vec<usize>,
        }

        fn inner_boundary_components<'a>(
            all_boundaries: &'a [Boundary],
            boundary: &Boundary,
        ) -> Vec<&'a Boundary> {
            let mut out = vec![];

            // TODO: Iterate over a vec of child index slices to avoid copies.
            let mut pending = boundary.children.clone();
            while let Some(id) = pending.pop() {
                let b = &all_boundaries[id];
                out.push(b);
                pending.extend_from_slice(&b.children);
            }

            out
        }

        let mut boundaries = vec![];
        let mut edge_to_boundary_index = HashMap::new();

        // Find all boundary cycles by traversing all the edges.
        for (edge_id, edge) in self.half_edges.iter() {
            if edge_to_boundary_index.contains_key(edge_id) {
                continue;
            }

            let mut edges = vec![];
            // let mut vertices = vec![];
            let mut self_faces = HashSet::new();

            let mut leftmost_vertex = *edge_id;

            // println!("===");

            {
                let mut current_id = *edge_id;
                while !edge_to_boundary_index.contains_key(&current_id) {
                    edges.push(current_id);
                    edge_to_boundary_index.insert(current_id, boundaries.len());

                    // if !self.half_edges.contains_key(&current_id) {
                    //     println!("missing id: {:?}", current_id);
                    // }

                    let edge = &self.half_edges[current_id];

                    self_faces.insert(edge.incident_face);
                    // vertices.push(edge.origin.clone());

                    // println!("{:?} @ V {:?}", current_id, edge.origin);

                    let current_leftmost = &self.half_edges[leftmost_vertex];

                    if compare_points_x_then_y(&edge.origin, &current_leftmost.origin).is_lt() {
                        leftmost_vertex = current_id;
                    }

                    current_id = edge.next;
                }

                // TODO: assert this this cycles through completely.
            }

            let is_inner = {
                let edge = &self.half_edges[leftmost_vertex];
                let next_edge = &self.half_edges[edge.next];
                let prev_edge = &self.half_edges[edge.prev];

                turns_right(&prev_edge.origin, &edge.origin, &next_edge.origin)
            };

            boundaries.push(Boundary {
                edges,
                // vertices,
                is_inner,
                leftmost_vertex,
                self_faces,
                // To be populated in the next loop.
                /// If this edge is an inner
                parent: None,

                children: vec![],
            });
        }

        // Link all inner boundaries to the boundary immediately to the link of them.
        for i in 0..boundaries.len() {
            let boundary = &boundaries[i];
            if !boundary.is_inner {
                continue;
            }

            let leftmost_edge = &self.half_edges[boundary.leftmost_vertex];

            let mut left_edge_id = *match edge_left_neighbors.get(&boundary.leftmost_vertex) {
                Some(v) => v,
                None => continue,
            };

            // The left neighbor may correspond to one of two faces (with the second one
            // associated with the twin of the neighbor).
            //
            // Based on the rule that the face lies to the LEFT of all
            // edges, we pick the parent which the
            // current boundary is actually inside of (based on the location of its leftmost
            // vertex).
            let parent_boundary_index = {
                let candidate_parent_index = edge_to_boundary_index[&left_edge_id];
                assert_ne!(candidate_parent_index, i);

                let mut left_edge = &self.half_edges[left_edge_id];
                let mut left_edge_dest = self.destination(left_edge);

                // If the left edge is horizontal, instead pick a non-horizontal one with the
                // same edge point as the right side of the horizontal line.
                // TODO: Use a standard constant
                if left_edge.origin.y() == left_edge_dest.y() {
                    // TODO: Implement a test case which hits thi logic.

                    println!("SKIP HORIZONTAL EDGE");

                    if left_edge.origin.x() > left_edge_dest.x() {
                        left_edge_id = left_edge.prev;
                    } else {
                        left_edge_id = left_edge.next;
                    }

                    left_edge = &self.half_edges[left_edge_id];
                    left_edge_dest = self.destination(left_edge);
                }

                let valid = {
                    let right_of_parent_edge =
                        turns_right(&left_edge.origin, &left_edge_dest, &leftmost_edge.origin);

                    !right_of_parent_edge
                };

                if valid {
                    candidate_parent_index
                } else {
                    edge_to_boundary_index[&left_edge.twin]
                }
            };

            assert_ne!(parent_boundary_index, i);

            boundaries[i].parent = Some(parent_boundary_index);
            boundaries[parent_boundary_index].children.push(i);
        }

        // Construct all faces.

        let mut faces = EntityStorage::new();

        let mut unbounded_face_id = faces.unique_id();
        let mut unbounded_face = Face {
            label: F::default(),
            outer_component: None,
            inner_components: vec![],
        };

        // TODO: Also implement transferring of data from the original faces.
        for boundary in &boundaries {
            if boundary.is_inner {
                if boundary.parent.is_some() {
                    // Handled by its parent.
                    continue;
                }

                // Otherwise, this is inside of the unbounded face.

                // TODO: Consider preserving the labels of unbounded faces (this would increase
                // the complexity of bounded faces though as we would need to search both inward
                // and outward for face references).

                unbounded_face
                    .inner_components
                    .push(boundary.leftmost_vertex);

                // TODO: When will this be non-zero? (two squares next to each other?)
                // assert_eq!(boundary.children.len(), 0);
                unbounded_face.inner_components.extend(
                    inner_boundary_components(&boundaries, boundary)
                        .into_iter()
                        .map(|b| {
                            for edge_id in &b.edges {
                                self.half_edges[*edge_id].incident_face = unbounded_face_id;
                            }

                            b.leftmost_vertex
                        }),
                );

                for edge_id in &boundary.edges {
                    self.half_edges[*edge_id].incident_face = unbounded_face_id;
                }

                // TODO: Loop over the edges to associate faces.
            } else {
                // Form a new face.

                let face_id = faces.unique_id();

                let mut included_faces = HashSet::<FaceId>::new();
                let mut excluded_faces = HashSet::<FaceId>::new();

                // TODO: Cache some of this computation so that each inner boundary doesn't need
                // to traverse up every single time.
                let mut current_edge = boundary.leftmost_vertex;

                bounded_loop(boundaries.len() + 1, || {
                    let mut boundary = &boundaries[edge_to_boundary_index[&current_edge]];

                    if !boundary.is_inner {
                        // When we encounter an outer boundary surrounding our boundary, we will
                        // inherit its labels. But,

                        for face in &boundary.self_faces {
                            if !excluded_faces.contains(face) {
                                included_faces.insert(*face);
                            }
                        }
                        // println!("INCLUDE {:?}", boundary.self_faces);

                        // Find the inner boundary surrounding the current bounary.
                        // TODO: Validate that this will at some point stop and doesn't go in loops.
                        // TODO: Should we be using is_inner of the new boundary or of the original
                        // boundary before we did the repairs?
                        // ^ Yes
                        bounded_loop(boundaries.len() + 1, || {
                            if boundary.is_inner {
                                return Ok(Loop::Break);
                            }

                            current_edge = self.half_edges[boundary.leftmost_vertex].twin;
                            boundary = &boundaries[edge_to_boundary_index[&current_edge]];
                            Ok(Loop::Continue)
                        })
                        .unwrap();

                        // Inner boundaries are hole components of faces, but because we know we are
                        // inside of the hole, we don't want to include any faces associated with
                        // the hole.
                        //
                        // NOTE: included_faces set should NOT yet have any of these newly excluded
                        // faces in it.
                        excluded_faces.extend(boundary.self_faces.clone());
                        // println!("EXCLUDE {:?}", boundary.self_faces);
                    } else {
                        // We encountered an inner (hole) component, jump up to the outer boundary.

                        let parent_idx = match boundary.parent.clone() {
                            Some(v) => v,
                            None => return Ok(Loop::Break),
                        };

                        let parent_boundary = &boundaries[parent_idx];
                        current_edge = parent_boundary.leftmost_vertex;
                    }

                    Ok(Loop::Continue)
                })
                .unwrap();

                let mut label = F::default();
                for id in included_faces {
                    label = label.union(&self.faces[id].label);
                }

                for edge_id in &boundary.edges {
                    self.half_edges[*edge_id].incident_face = face_id;
                }

                faces.insert(
                    face_id,
                    Face {
                        label,
                        outer_component: Some(boundary.leftmost_vertex),
                        inner_components: inner_boundary_components(&boundaries, boundary)
                            .into_iter()
                            .map(|b| {
                                for edge_id in &b.edges {
                                    self.half_edges[*edge_id].incident_face = face_id;
                                }

                                b.leftmost_vertex
                            })
                            .collect(),
                    },
                );
            }
        }

        faces.insert(unbounded_face_id, unbounded_face);

        self.faces = faces;
        self.unbounded_face_id = unbounded_face_id;
    }

    /// Assuming this data structure is valid accounting to repair(), then this
    /// will further rewrite this data structure to consist of only y-monotone
    /// faces (splitting existing faces as appropriate).
    pub fn make_y_monotone(&mut self) {
        let mut face_ids = self.faces.keys().cloned().collect::<Vec<_>>();

        // Should we just do everything in one pass?
        for face_id in face_ids {
            if face_id == self.unbounded_face_id {
                continue;
            }

            self.make_y_monotone_face(face_id);
        }
    }

    fn make_y_monotone_face(&mut self, face_id: FaceId) {
        let face = &self.faces[face_id];

        let mut line_segments = vec![];
        let mut line_segments_to_edge = vec![];

        // Extract line segments from all edges.
        for component_id in face
            .outer_component
            .iter()
            .chain(face.inner_components.iter())
        {
            // TODO: Consider always storing the min id edge in the face components so we
            // can gurantee that this will halt (or strictly reach higher edge ids).
            let mut current_id = *component_id;
            bounded_loop(self.half_edges.len() + 1, || {
                let edge = &self.half_edges[current_id];

                line_segments.push(LineSegment2 {
                    start: dequantize2(edge.origin.clone()),
                    end: dequantize2(self.destination(edge)),
                });
                line_segments_to_edge.push(current_id);

                if edge.next == *component_id {
                    return Ok(Loop::Break);
                }

                current_id = edge.next;

                Ok(Loop::Continue)
            })
            .unwrap();
        }

        #[derive(Debug)]
        enum VertexType {
            Start,
            Split,
            Merge,
            End,
            Regular,
        }

        let mut lowest_interior_points = HashMap::new();

        // TODO: For this we should use exact matching of intersections.

        let intersections = LineSegment2::intersections(&line_segments, 1e-3);

        // let mut intersection_starts = vec![];
        // for intersection in &intersections {
        //     for segment in intersection.segments.iter().cloned() {
        //         let edge_id = line_segments_to_edge[intersection.segments[0]];
        //         let edge = &self.half_edges[edge_id];
        //         if edge.origin == quantize2(intersection.point.clone()) {
        //             intersection_starts.push((edge_id, intersection));
        //         }

        //         // assert!(edge.origin == quantize2(intersection.point));
        //     }
        // }

        // Iterate over vertices in the face (as all our faces should be closed, this
        // corresponds to each intersection point too).
        //
        // TODO: Execute that at the same time as the repair() process.
        for intersection in intersections {
            if intersection.segments.len() != 2 {
                println!("{:#?}", intersection);
            }

            // Always true as we are only considering a single face at a time.
            assert_eq!(intersection.segments.len(), 2);

            // let prev_edge_id = self.half_edges[edge_id].prev;

            // Id of the edge originating at the intersection point and the one before it.
            let (edge_id, prev_edge_id) = {
                let a = line_segments_to_edge[intersection.segments[0]];
                let b = line_segments_to_edge[intersection.segments[1]];

                if self.half_edges[a].prev == b {
                    (a, b)
                } else {
                    (b, a)
                }
            };

            let edge = &self.half_edges[edge_id];
            assert!(edge.origin == quantize2(intersection.point));

            let neighbor1 = self.half_edges[prev_edge_id].origin.clone();
            let neighbor2 = self.destination(&edge);

            // We saw that our neighbor is 'below' the current vertex if we haven't yet seen
            // it while scanning for intersections.
            let neighbor1_below = compare_points_i64(&edge.origin, &neighbor1).is_lt();
            let neighbor2_below = compare_points_i64(&edge.origin, &neighbor2).is_lt();

            // If true, then the interior angle at this vertex is > PI
            let big_interior_angle = turns_right(&neighbor1, &edge.origin, &neighbor2);

            if neighbor1_below && neighbor2_below {
                if !big_interior_angle {
                    // Start vertex
                    lowest_interior_points.insert(edge_id, (edge_id, VertexType::Start));
                } else {
                    // Split vertex
                    // A left neighbor should always exist for this. Otherwise we would be a 'start'
                    // vertex
                    let left_edge = line_segments_to_edge[intersection.left_neighbor.unwrap()];
                    self.connect_face_vertices(edge_id, lowest_interior_points[&left_edge].0);
                    lowest_interior_points.insert(left_edge, (edge_id, VertexType::Split));
                }
            } else if !neighbor1_below && !neighbor2_below {
                if !big_interior_angle {
                    // End vertex
                    if let Some((merge_edge_id, VertexType::Merge)) =
                        lowest_interior_points.get(&prev_edge_id)
                    {
                        self.connect_face_vertices(edge_id, *merge_edge_id);
                    }
                } else {
                    // Merge vertex
                    // A left neighbor should always exist. Otherwise we would be an 'end' vertex.

                    if let Some((merge_edge_id, VertexType::Merge)) =
                        lowest_interior_points.get(&prev_edge_id)
                    {
                        self.connect_face_vertices(edge_id, *merge_edge_id);
                    }

                    let left_edge = line_segments_to_edge[intersection.left_neighbor.unwrap()];
                    if let Some((merge_edge_id, VertexType::Merge)) =
                        lowest_interior_points.get(&left_edge)
                    {
                        self.connect_face_vertices(edge_id, *merge_edge_id);
                    }
                    lowest_interior_points.insert(left_edge, (edge_id, VertexType::Merge));
                }
            } else {
                // Regular vertex

                // TODO: For horizontal lines in holes, the x comparison should be inverted
                // (from > to <).
                let interior_on_right = {
                    let dir = &neighbor2 - &edge.origin;
                    dir.y() < 0 || (dir.y() == 0 && dir.x() > 0)
                };

                if interior_on_right {
                    if let Some((merge_edge_id, VertexType::Merge)) =
                        lowest_interior_points.get(&prev_edge_id)
                    {
                        self.connect_face_vertices(edge_id, *merge_edge_id);
                    }

                    lowest_interior_points.insert(edge_id, (edge_id, VertexType::Regular));
                } else {
                    // TODO: Check this

                    let left_edge = line_segments_to_edge[intersection.left_neighbor.unwrap()];
                    if let Some((merge_edge_id, VertexType::Merge)) =
                        lowest_interior_points.get(&left_edge)
                    {
                        self.connect_face_vertices(edge_id, *merge_edge_id);
                    }
                    lowest_interior_points.insert(left_edge, (edge_id, VertexType::Regular));
                }
            }
        }
    }

    /// Connects two vertices of a single face with a new line segment.
    ///
    /// In particular, each of the given edge ids defines a point at each edge's
    /// origin that will be used. Only the prev pointers of the given edges will
    /// be modified (the next edges will stay the same).
    ///
    /// Assumptions:
    /// - vertex_a and vertex_b belong to the same face.
    /// - A line can be drawn from vertex_a to vertex
    ///
    /// NOTE: If both edges aren't on the same boundary component, then the face
    /// boundary records will be invalid after this operation.
    fn connect_face_vertices(&mut self, vertex_a: EdgeId, vertex_b: EdgeId) {
        // TODO: Assert vertex edges are from the face same.

        assert_ne!(vertex_a, vertex_b);

        // println!("Connect {:?} {:?}", vertex_a, vertex_b);

        let id1 = self.half_edges.unique_id();
        let id2 = self.half_edges.unique_id();

        let edge_a = &mut self.half_edges[vertex_a];
        let edge_a_old_prev = edge_a.prev;
        let edge_a_origin = edge_a.origin.clone();
        let edge_a_face = edge_a.incident_face;
        edge_a.prev = id1;
        self.half_edges[edge_a_old_prev].next = id2;

        // TODO: Deduplicate with above.
        let edge_b = &mut self.half_edges[vertex_b];
        let edge_b_old_prev = edge_b.prev;
        let edge_b_origin = edge_b.origin.clone();
        assert_eq!(edge_a_face, edge_b.incident_face);
        edge_b.prev = id2;
        self.half_edges[edge_b_old_prev].next = id1;

        // println!("CONNECT {:?} => {:?}", edge_a_origin, edge_b_origin);

        self.half_edges.insert(
            id1,
            HalfEdge {
                origin: edge_b_origin,
                twin: id2,
                incident_face: edge_a_face,
                next: vertex_a,
                prev: edge_b_old_prev,
            },
        );

        self.half_edges.insert(
            id2,
            HalfEdge {
                origin: edge_a_origin,
                twin: id1,
                incident_face: edge_a_face,
                next: vertex_b,
                prev: edge_a_old_prev,
            },
        );
    }

    pub fn triangulate_monotone(&mut self) {
        let mut face_ids = self.faces.keys().cloned().collect::<Vec<_>>();

        // Should we just do everything in one pass?
        for face_id in face_ids {
            if face_id == self.unbounded_face_id {
                continue;
            }

            self.triangulate_monotone_face(face_id);
        }

        // TODO: Now all faces should be triangles. We should try to optimize
        // the angles If we two adjacent triangles, consider them to be
        // a quadrilateral and try to swap the diagonals to see if that makes
        // angles less extreme.
    }

    fn triangulate_monotone_face(&mut self, face_id: FaceId) {
        let edges = {
            let face = &self.faces[face_id];
            // Faces with holes are not monotone.
            assert!(face.inner_components.is_empty());

            let mut edges = vec![];

            let first_id = face.outer_component.unwrap();
            let mut current_id = first_id;
            loop {
                edges.push(current_id);
                current_id = self.half_edges[current_id].next;
                if current_id == first_id {
                    break;
                }
            }

            edges.sort_by(|a, b| {
                compare_points_i64(&self.half_edges[*a].origin, &self.half_edges[*b].origin)
            });

            edges
        };

        let mut stack = vec![];
        // TODO: Assert these are on the same side.
        stack.push(edges[0]);
        stack.push(edges[1]);

        for i in 2..(edges.len() - 1) {
            let v_i = edges[i];

            /*
            For two vertices to be on different sides,
            */

            // NOTE: We don't compare edge ids as the connect_face_vertices() function will
            // have messed up the connectivity of any vertices on the right boundary.
            let (on_same_side, on_left) = {
                let a = &self.half_edges[v_i];
                let b = &self.half_edges[*stack.last().unwrap()];

                // Will be true if both vertices are on the left side of the face.
                let left = self.destination(b) == a.origin;

                // Will be true if both vertices are on the right side of the face.
                let right = self.destination(a) == b.origin;

                (left || right, left)
            };

            // TODO: adding diagonals will probably mess up this direction.
            if !on_same_side {
                // assert stack[0] is connected to v_i

                // assert!(self.half_edges[stack[0]].next == v_i);

                for v_j in &stack[1..] {
                    self.connect_face_vertices(v_i, *v_j);
                }

                assert_eq!(edges[i - 1], *stack.last().unwrap());

                stack.clear();
                stack.push(edges[i - 1]);
                stack.push(v_i);
            } else {
                // The current vertex should be connected to this one.
                let mut last_vertex = stack.pop().unwrap();

                while let Some(next_vertex) = stack.last().cloned() {
                    // TODO: Ensure that we do not connect three points that are all co-linear.

                    // We can only insert a diagonal if the line would be inside of the face.
                    // Note that the face on the left side of edges.
                    //
                    // Also note that last_vertex should be connected to next_vertext.
                    if turns_right(
                        &self.half_edges[v_i].origin,
                        &self.half_edges[last_vertex].origin,
                        &self.half_edges[next_vertex].origin,
                    ) == !on_left
                    {
                        break;
                    }

                    self.connect_face_vertices(v_i, next_vertex);

                    last_vertex = next_vertex;
                    stack.pop();
                }

                stack.push(last_vertex);
                stack.push(v_i);
            }
        }

        for edge in &stack[1..(stack.len() - 1)] {
            self.connect_face_vertices(edges[edges.len() - 1], *edge);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FaceDebug<F> {
    pub label: F,
    pub outer_component: Option<Vec<Vector2f>>,
    pub inner_components: Vec<Vec<Vector2f>>,
}

impl<F: FaceLabel> FaceDebug<F> {
    // Validates the correctness of the HalfEdgeStruct and extracts all boundary
    // cycles starting at any edges.
    pub fn get_all(data: &HalfEdgeStruct<F>) -> Vec<Self> {
        let mut output = vec![];

        let mut seen_ids = HashSet::new();

        for (face_id, face) in data.faces.iter() {
            let mut outer_component = None;

            if let Some(first_edge_id) = &face.outer_component {
                outer_component = Some(Self::traverse_cycle(
                    data,
                    *face_id,
                    *first_edge_id,
                    &mut seen_ids,
                ));
            }

            let mut inner_components = vec![];
            for first_edge_id in &face.inner_components {
                inner_components.push(Self::traverse_cycle(
                    data,
                    *face_id,
                    *first_edge_id,
                    &mut seen_ids,
                ));
            }

            output.push(Self {
                label: face.label.clone(),
                outer_component,
                inner_components,
            });
        }

        for (edge_id, edge) in data.half_edges.iter() {
            assert_eq!(data.half_edges[edge.next].prev, *edge_id);
            assert_eq!(data.half_edges[edge.prev].next, *edge_id);
            assert_eq!(data.half_edges[edge.twin].twin, *edge_id);
            assert!(seen_ids.contains(edge_id));

            // Edges along a boundary should all be pointing in the same direction.
            let prev_dest = data.destination(&data.half_edges[edge.prev]);
            assert_eq!(prev_dest, edge.origin);
        }

        output
    }

    fn traverse_cycle(
        data: &HalfEdgeStruct<F>,
        face_id: FaceId,
        first_edge_id: EdgeId,
        seen_ids: &mut HashSet<EdgeId>,
    ) -> Vec<Vector2f> {
        let mut boundary = vec![];
        let mut current_id = first_edge_id;
        while seen_ids.insert(current_id) {
            let current_edge = &data.half_edges[current_id];
            assert_eq!(current_edge.incident_face, face_id);
            boundary.push(dequantize2(current_edge.origin.clone()));
            current_id = current_edge.next;
        }

        // Must have wrapped around.
        assert_eq!(current_id, first_edge_id);

        boundary
    }
}

/*
Things we should do first:
- Eliminate any edges of length 0
- Merge connected line segments which are co-linear within some threshold.

*/

// unordered_elements_are()

#[cfg(test)]
mod tests {
    use super::*;

    use testing::*;

    #[test]
    fn two_lines_intersect() {
        let mut data = HalfEdgeStruct::<()>::new();

        let e1 = data.half_edges.unique_id();
        let e2 = data.half_edges.unique_id();
        let e3 = data.half_edges.unique_id();
        let e4 = data.half_edges.unique_id();

        data.half_edges.insert(
            e1,
            HalfEdge {
                origin: quantize2(vec2f(0., 0.)),
                twin: e2,
                next: e2,
                prev: e2,
                incident_face: data.unbounded_face_id,
            },
        );
        data.half_edges.insert(
            e2,
            HalfEdge {
                origin: quantize2(vec2f(10., 10.)),
                twin: e1,
                next: e1,
                prev: e1,
                incident_face: data.unbounded_face_id,
            },
        );
        data.half_edges.insert(
            e3,
            HalfEdge {
                origin: quantize2(vec2f(10., 0.)),
                twin: e4,
                next: e4,
                prev: e4,
                incident_face: data.unbounded_face_id,
            },
        );
        data.half_edges.insert(
            e4,
            HalfEdge {
                origin: quantize2(vec2f(0., 10.)),
                twin: e3,
                next: e3,
                prev: e3,
                incident_face: data.unbounded_face_id,
            },
        );

        data.repair();

        // Should be an inner boundary with 8 edges going clockwise around the surface
        // of the two lines.
        assert_eq!(
            &FaceDebug::get_all(&data),
            &[FaceDebug {
                label: (),
                outer_component: None,
                inner_components: vec![vec![
                    vec2f(0.0, 0.0),
                    vec2f(5.0, 5.0),
                    vec2f(0.0, 10.0),
                    vec2f(5.0, 5.0),
                    vec2f(10.0, 10.0),
                    vec2f(5.0, 5.0),
                    vec2f(10.0, 0.0),
                    vec2f(5.0, 5.0),
                ]]
            }]
        );
    }

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
    fn repair_unclosed_polygon() {
        let mut data = HalfEdgeStruct::new();

        // Note that this label will be discarded as we don't preserve labels of
        // unbounded faces.
        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));

        // The edge utilities will optimistically create two faces although there is
        // really still just one face at this point.
        assert_eq!(data.faces.len(), 2);

        data.repair();

        assert_eq!(
            &FaceDebug::get_all(&data),
            &[FaceDebug {
                label: HashSet::new(),
                outer_component: None,
                inner_components: vec![vec![
                    vec2f(0.0, 0.0),
                    vec2f(10.0, 0.0),
                    vec2f(10.0, 10.0),
                    vec2f(10.0, 0.0),
                ],],
            },]
        );
    }

    #[test]
    fn repair_self_intersecting() {
        //           |\
        //           | \
        //           |  \
        //           |   \
        //           |   /
        //           |  /
        //           | /
        //           |/
        //          /
        //         / |
        //        /  |
        //       /   |
        //      /    |
        //     /     |
        //    /      |
        //   /       |
        //   ---------

        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(10., 20.));
        let a2 = data.add_next_edge(a1, vec2f(20., 15.));
        data.add_close_edge(a2, a0);

        data.repair();

        assert_that(
            &FaceDebug::get_all(&data),
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: HashSet::default(),
                    outer_component: Some(vec![
                        vec2f(10.0, 7.5),
                        vec2f(20.0, 15.0),
                        vec2f(10.0, 20.0),
                    ]),
                    inner_components: vec![],
                }),
                // NOTE: The face is actually to the right of this shape, so the label is not
                // inherited.
                eq(FaceDebug {
                    label: HashSet::default(),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(10.0, 7.5),
                        vec2f(10.0, 20.0),
                        vec2f(20.0, 15.0),
                        vec2f(10.0, 7.5),
                        vec2f(10.0, 0.0),
                    ]],
                }),
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(10.0, 7.5),
                    ]),
                    inner_components: vec![],
                }),
            ]),
        );
    }

    #[test]
    fn repair_noop_for_closed_triangle() {
        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("T"));
        let a1 = data.add_next_edge(a0, vec2f(5., 5.));
        data.add_close_edge(a1, a0);

        data.repair();

        assert_that(
            &FaceDebug::get_all(&data),
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: HashSet::default(),
                    outer_component: None,
                    inner_components: vec![vec![vec2f(0.0, 0.), vec2f(5.0, 5.0), vec2f(10.0, 0.0)]],
                }),
                eq(FaceDebug {
                    label: label("T"),
                    outer_component: Some(vec![vec2f(0.0, 0.), vec2f(10.0, 0.0), vec2f(5.0, 5.0)]),
                    inner_components: vec![],
                }),
            ]),
        );
    }

    #[test]
    fn two_squares_intersect() {
        //    -------
        //    |     |
        // ---+--   |
        // |  --+----
        // |    |
        // ------

        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));
        let a2 = data.add_next_edge(a1, vec2f(0., 10.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(5., 5.), vec2f(15., 5.), label("B"));
        let b1 = data.add_next_edge(b0, vec2f(15., 15.));
        let b2 = data.add_next_edge(b1, vec2f(5., 15.));
        data.add_close_edge(b2, b0);

        data.repair();

        assert_eq!(data.half_edges.len(), 24);

        assert_that(
            &FaceDebug::get_all(&data),
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: HashSet::new(),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 10.0),
                        vec2f(5.0, 10.0),
                        vec2f(5.0, 15.0),
                        vec2f(15.0, 15.0),
                        vec2f(15.0, 5.0),
                        vec2f(10.0, 5.0),
                        vec2f(10.0, 0.0),
                    ]],
                }),
                // Lower square with overlap carved out
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(10.0, 5.0),
                        vec2f(5.0, 5.0),
                        vec2f(5.0, 10.0),
                        vec2f(0.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
                // Upper square with overlap carved out
                eq(FaceDebug {
                    label: label("B"),
                    outer_component: Some(vec![
                        vec2f(5.0, 10.0),
                        vec2f(10.0, 10.0),
                        vec2f(10.0, 5.0),
                        vec2f(15.0, 5.0),
                        vec2f(15.0, 15.0),
                        vec2f(5.0, 15.0),
                    ]),
                    inner_components: vec![],
                }),
                // Middle overlap
                eq(FaceDebug {
                    label: labels(&["A", "B"]),
                    outer_component: Some(vec![
                        vec2f(5.0, 5.0),
                        vec2f(10.0, 5.0),
                        vec2f(10.0, 10.0),
                        vec2f(5.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
            ]),
        );
    }

    #[test]
    fn square_inside_square_test() {
        // ------------------|
        // |                 |
        // |  ------------   |
        // |  |          |   |
        // |  |          |   |
        // |  |          |   |
        // |  ------------   |
        // |                 |
        // -------------------

        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(20., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(20., 20.));
        let a2 = data.add_next_edge(a1, vec2f(0., 20.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(5., 5.), vec2f(15., 5.), label("B"));
        let b1 = data.add_next_edge(b0, vec2f(15., 15.));
        let b2 = data.add_next_edge(b1, vec2f(5., 15.));
        data.add_close_edge(b2, b0);

        data.repair();

        let boundaries = FaceDebug::get_all(&data);

        assert_that(
            &boundaries,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: HashSet::new(),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 20.0),
                        vec2f(20.0, 20.0),
                        vec2f(20.0, 0.0),
                    ]],
                }),
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(20.0, 0.0),
                        vec2f(20.0, 20.0),
                        vec2f(0.0, 20.0),
                    ]),
                    inner_components: vec![vec![
                        vec2f(5.0, 5.0),
                        vec2f(5.0, 15.0),
                        vec2f(15.0, 15.0),
                        vec2f(15.0, 5.0),
                    ]],
                }),
                eq(FaceDebug {
                    label: labels(&["A", "B"]),
                    outer_component: Some(vec![
                        vec2f(5.0, 5.0),
                        vec2f(15.0, 5.0),
                        vec2f(15.0, 15.0),
                        vec2f(5.0, 15.0),
                    ]),
                    inner_components: vec![],
                }),
            ]),
        );

        ////////////////

        data.make_y_monotone();
        data.repair();

        let boundaries = FaceDebug::get_all(&data);
        assert_that(
            &boundaries,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: HashSet::new(),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 20.0),
                        vec2f(20.0, 20.0),
                        vec2f(20.0, 0.0),
                    ]],
                }),
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(15.0, 5.0),
                        vec2f(5.0, 5.0),
                        vec2f(5.0, 15.0),
                        vec2f(20.0, 20.0),
                        vec2f(0.0, 20.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(20.0, 0.0),
                        vec2f(20.0, 20.0),
                        vec2f(5.0, 15.0),
                        vec2f(15.0, 15.0),
                        vec2f(15.0, 5.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: labels(&["A", "B"]),
                    outer_component: Some(vec![
                        vec2f(5.0, 5.0),
                        vec2f(15.0, 5.0),
                        vec2f(15.0, 15.0),
                        vec2f(5.0, 15.0),
                    ]),
                    inner_components: vec![],
                }),
            ]),
        );

        println!("Triangulate!");
        data.triangulate_monotone();
        println!("Done");

        data.repair();
        println!("Repairing done!");

        let boundaries = FaceDebug::get_all(&data);
        // println!("{:#?}", boundaries);
    }

    #[test]
    fn square_inside_square_stable() {
        // If the inner square and outer square have different labels, they
        // should not change after a repeair.
        // TODO
    }

    #[test]
    fn square_above_square() {
        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(20., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(20., 40.));
        let a2 = data.add_next_edge(a1, vec2f(0., 40.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(5., 5.), vec2f(15., 5.), label("B"));
        let b1 = data.add_next_edge(b0, vec2f(15., 15.));
        let b2 = data.add_next_edge(b1, vec2f(5., 15.));
        data.add_close_edge(b2, b0);

        let c0 = data.add_first_edge(vec2f(5., 25.), vec2f(15., 25.), label("C"));
        let c1 = data.add_next_edge(c0, vec2f(15., 35.));
        let c2 = data.add_next_edge(c1, vec2f(5., 35.));
        data.add_close_edge(c2, c0);

        data.repair();

        println!("MAKE MONOTONE!");

        data.make_y_monotone();

        //
    }

    #[test]
    fn adjacent_shifted_squares() {
        //          ------
        //          |    |
        // ------   |    |
        // |    |   ------
        // |    |
        // ------

        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));
        let a2 = data.add_next_edge(a1, vec2f(0., 10.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(15., 5.), vec2f(25., 5.), label("B"));
        let b1 = data.add_next_edge(b0, vec2f(25., 15.));
        let b2 = data.add_next_edge(b1, vec2f(15., 15.));
        data.add_close_edge(b2, b0);

        data.repair();

        let boundaries = FaceDebug::get_all(&data);
        // println!("{:#?}", boundaries);
        assert_that(
            &boundaries,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![
                        vec![
                            vec2f(0.0, 0.0),
                            vec2f(0.0, 10.0),
                            vec2f(10.0, 10.0),
                            vec2f(10.0, 0.0),
                        ],
                        vec![
                            vec2f(15.0, 5.0),
                            vec2f(15.0, 15.0),
                            vec2f(25.0, 15.0),
                            vec2f(25.0, 5.0),
                        ],
                    ],
                }),
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(10.0, 10.0),
                        vec2f(0.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: label("B"),
                    outer_component: Some(vec![
                        vec2f(15.0, 5.0),
                        vec2f(25.0, 5.0),
                        vec2f(25.0, 15.0),
                        vec2f(15.0, 15.0),
                    ]),
                    inner_components: vec![],
                }),
            ]),
        );
    }

    #[test]
    fn merge_partially_overlapping_edges() {
        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));
        let a2 = data.add_next_edge(a1, vec2f(0., 10.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(10., 5.), vec2f(20., 5.), label("B"));
        let b1 = data.add_next_edge(b0, vec2f(20., 15.));
        let b2 = data.add_next_edge(b1, vec2f(10., 15.));
        data.add_close_edge(b2, b0);

        data.repair();

        let boundaries = FaceDebug::get_all(&data);
        // println!("{:#?}", boundaries);
        assert_that(
            &boundaries,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(10.0, 5.0),
                        vec2f(10.0, 10.0),
                        vec2f(0.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: label("B"),
                    outer_component: Some(vec![
                        vec2f(10.0, 5.0),
                        vec2f(20.0, 5.0),
                        vec2f(20.0, 15.0),
                        vec2f(10.0, 15.0),
                        vec2f(10.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 10.0),
                        vec2f(10.0, 10.0),
                        vec2f(10.0, 15.0),
                        vec2f(20.0, 15.0),
                        vec2f(20.0, 5.0),
                        vec2f(10.0, 5.0),
                        vec2f(10.0, 0.0),
                    ]],
                }),
            ]),
        );
    }

    #[test]
    fn merge_completely_overlapping_edges() {
        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));
        let a2 = data.add_next_edge(a1, vec2f(0., 10.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(10., 0.), vec2f(20., 0.), label("B"));
        let b1 = data.add_next_edge(b0, vec2f(20., 10.));
        let b2 = data.add_next_edge(b1, vec2f(10., 10.));
        data.add_close_edge(b2, b0);

        data.repair();

        let boundaries = FaceDebug::get_all(&data);
        // println!("{:#?}", boundaries);
        assert_that(
            &boundaries,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(10.0, 10.0),
                        vec2f(0.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: label("B"),
                    outer_component: Some(vec![
                        vec2f(10.0, 0.0),
                        vec2f(20.0, 0.0),
                        vec2f(20.0, 10.0),
                        vec2f(10.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 10.0),
                        vec2f(10.0, 10.0),
                        vec2f(20.0, 10.0),
                        vec2f(20.0, 0.0),
                        vec2f(10.0, 0.0),
                    ]],
                }),
            ]),
        );
    }

    #[test]
    fn merge_multiple_overlapping_edges() {
        // TODO: this is an intersecting example as a deleted edge adds labels to
        // multiple labels later on after it is deleted.

        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));
        let a2 = data.add_next_edge(a1, vec2f(0., 10.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(10., 0.), vec2f(20., 0.), label("B"));
        let b1 = data.add_next_edge(b0, vec2f(20., 10.));
        let b2 = data.add_next_edge(b1, vec2f(10., 10.));
        data.add_close_edge(b2, b0);

        let c0 = data.add_first_edge(vec2f(5., 10.), vec2f(15., 10.), label("C"));
        let c1 = data.add_next_edge(c0, vec2f(15., 20.));
        let c2 = data.add_next_edge(c1, vec2f(5., 20.));
        data.add_close_edge(c2, c0);

        data.repair();

        let boundaries = FaceDebug::get_all(&data);
        // println!("{:#?}", boundaries);

        assert_that(
            &boundaries,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(10.0, 10.0),
                        vec2f(5.0, 10.0),
                        vec2f(0.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: label("B"),
                    outer_component: Some(vec![
                        vec2f(10.0, 0.0),
                        vec2f(20.0, 0.0),
                        vec2f(20.0, 10.0),
                        vec2f(15.0, 10.0),
                        vec2f(10.0, 10.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: label("C"),
                    outer_component: Some(vec![
                        vec2f(5.0, 10.0),
                        vec2f(10.0, 10.0),
                        vec2f(15.0, 10.0),
                        vec2f(15.0, 20.0),
                        vec2f(5.0, 20.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 10.0),
                        vec2f(5.0, 10.0),
                        vec2f(5.0, 20.0),
                        vec2f(15.0, 20.0),
                        vec2f(15.0, 10.0),
                        vec2f(20.0, 10.0),
                        vec2f(20.0, 0.0),
                        vec2f(10.0, 0.0),
                    ]],
                }),
            ]),
        );
    }

    #[test]
    fn delete_new_overlapping_edge() {
        // Depending on how we sort partial edges at the (3, 0) intersection point, we
        // may decide to 'delete' the brand new edge from '(3, 0) -> (10, 0)'. If this
        // edge doesn't end up appearing in the half_edges struct, then we will get
        // failures as it still needs to be considered when calculating the (10, 0)
        // intersection.

        {
            let mut data = HalfEdgeStruct::new();
            let a = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
            let b = data.add_first_edge(vec2f(3., 0.), vec2f(10., 0.), label("B"));
            let c = data.add_first_edge(vec2f(5., 0.), vec2f(15., 0.), label("C"));

            data.repair();

            let boundaries = FaceDebug::get_all(&data);
            assert_that(
                &boundaries,
                unordered_elements_are(&[eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(3.0, 0.0),
                        vec2f(5.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(15.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(5.0, 0.0),
                        vec2f(3.0, 0.0),
                    ]],
                })]),
            );
        }

        // Same thing as above but with inverse insertion order
        {
            let mut data = HalfEdgeStruct::new();
            let c = data.add_first_edge(vec2f(5., 0.), vec2f(15., 0.), label("C"));
            let b = data.add_first_edge(vec2f(3., 0.), vec2f(10., 0.), label("B"));
            let a = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));

            data.repair();

            let boundaries = FaceDebug::get_all(&data);
            assert_that(
                &boundaries,
                unordered_elements_are(&[eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(3.0, 0.0),
                        vec2f(5.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(15.0, 0.0),
                        vec2f(10.0, 0.0),
                        vec2f(5.0, 0.0),
                        vec2f(3.0, 0.0),
                    ]],
                })]),
            );
        }
    }

    #[test]
    fn complex_merge() {
        // There's an overlap at (0,0) and (1,1)

        let points = &[
            vec2f(0., 0.),
            vec2f(10., 0.),
            vec2f(10., 10.),
            vec2f(0., 10.),
            vec2f(0., 0.),
            vec2f(1., 1.),
            vec2f(9., 1.),
            vec2f(9., 9.),
            vec2f(1., 9.),
            vec2f(1., 1.),
            // vec2f(187.11684, 340.03354),
            // vec2f(320.91425, 343.99792),
            // vec2f(432.4529, 359.932),
            // vec2f(447.96048, 380.28568),
            // vec2f(426.11734, 569.92377),
            // vec2f(209.90596, 552.0714),
            // vec2f(187.11684, 340.03354),
            // vec2f(184.88315, 337.96646),
            // vec2f(208.09404, 553.9286),
            // vec2f(427.88266, 572.07623),
            // vec2f(450.03952, 379.71432),
            // vec2f(433.5471, 358.068),
            // vec2f(321.08575, 342.0021),
            // vec2f(184.88315, 337.96646),
        ];

        let mut data = HalfEdgeStruct::new();

        let first = data.add_first_edge(points[0].clone(), points[1].clone(), labels(&[]));
        let mut next = data.add_next_edge(first, points[2].clone());
        for p in &points[3..] {
            next = data.add_next_edge(next, p.clone());
        }
        data.add_close_edge(next, first);

        data.repair();

        for (_, e) in data.half_edges.iter() {
            println!("{:?}", e.origin);
        }

        let boundaries = FaceDebug::get_all(&data);
        println!("{:#?}", boundaries);

        /*
        For every vertex, st

        */

        // data.repair();
        data.make_y_monotone();
    }

    // TODO: Test for ignoring line segments with length 0 (and pruning them
    // from the structure).

    // TODO: Test making a square with two square holes stacked vertically with
    // some gap into a monotone shape.
}
