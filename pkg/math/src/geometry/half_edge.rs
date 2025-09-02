use alloc::vec::Vec;
use common::hash::FastHasherBuilder;
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
use crate::matrix::Dimension;
use crate::matrix::Vector2;
use crate::matrix::Vector2i64;
use crate::matrix::{vec2f, Vector2f};
use crate::number::Zero;
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



TODOs:
- Need resilience to having multiple edges which use duplicate start/end points.

*/

pub trait FaceLabel: Clone + Default + Debug + PartialEq {
    // TODO: Maybe use BitOr instead?
    fn union(&self, other: &Self) -> Self;
}

impl FaceLabel for () {
    fn union(&self, other: &Self) -> Self {
        ()
    }
}

impl FaceLabel for bool {
    fn union(&self, other: &Self) -> Self {
        *self || *other
    }
}

impl<T: Clone + Debug + Hash + PartialEq + Eq> FaceLabel for HashSet<T> {
    fn union(&self, other: &Self) -> Self {
        self | other
    }
}

/// Half edge / doubly conencted edge list data structure for storing a set of
/// set of faces subdividing a 2D surface.
///
/// - Edges are stored as two 'twin' oriented half-edges.
///   - e.g between points A and B, there are half edges 'A -> B' and 'B -> A'.
/// - Half edges are chained together in a cycle boundaries of faces.
///     - Since faces may have holes, one face may have multiple boundaries but
///       just one outer boundary.
/// - Faces are to the 'left' of each half edge
///   - Outer boundary/component half-edge cycles are stored in
///     counter-clockwise order.
///   - Cycles representing 'holes' are stored in clockwise order.
#[derive(Debug, Clone)]
pub struct HalfEdgeStruct<F> {
    half_edges: EntityStorage<EdgeTag, HalfEdge>,
    faces: EntityStorage<FaceTag, Face<F>>,
    unbounded_face_id: FaceId,
    scale: f32,
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
    /// Creates a new empty struct containing new edges.
    pub fn new() -> Self {
        Self::new_with_scale(DEFAULT_SCALE)
    }

    pub fn new_with_scale(scale: f32) -> Self {
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
            scale,
        }
    }

    pub fn faces<'a>(&'a self) -> FacesIterator<'a, F> {
        FacesIterator {
            inst: self,
            faces: self.faces.iter(),
        }
    }

    pub fn num_faces(&self) -> usize {
        self.faces.len()
    }

    pub fn num_half_edges(&self) -> usize {
        self.half_edges.len()
    }

    pub fn add_face<I: Iterator<Item = Vector2f>>(&mut self, label: F, mut points: I) {
        // TODO: Prevent this from adding zero area faces (must have at least three
        // distinct vertices)

        let mut face_id = self.faces.unique_id();
        let mut other_face_id = self.faces.unique_id();

        let mut last_point = match points.next() {
            Some(v) => quantize2(v, self.scale),
            None => return,
        };

        let mut first_edge = None;

        let mut last_edge = None;

        let mut leftmost_vertex = None;

        // Add edges going to each next point.
        // Note: The last iteration of this will re-visit the first point to add the
        // closing edge.
        let scale = self.scale;
        let mut points_iter = points
            .map(|p| quantize2(p, scale))
            .chain(std::iter::once(last_point.clone()));
        while let Some(point) = points_iter.next() {
            if point == last_point {
                continue;
            }

            let id = self.half_edges.unique_id();
            let twin = self.half_edges.unique_id();

            if let Some(prev) = last_edge {
                // Create a new edge connecting to the previous edge.

                let prev_twin = self.half_edges[prev].twin;

                self.half_edges.insert(
                    id,
                    HalfEdge {
                        origin: last_point.clone(),
                        twin,
                        incident_face: face_id,
                        next: twin,
                        prev,
                    },
                );
                self.half_edges[prev].next = id;

                self.half_edges.insert(
                    twin,
                    HalfEdge {
                        origin: point.clone(),
                        twin: id,
                        incident_face: other_face_id,
                        next: prev_twin,
                        prev: id,
                    },
                );
                self.half_edges[prev_twin].prev = twin;
            } else {
                // Creating the first edge.

                first_edge = Some(id);

                self.half_edges.insert(
                    id,
                    HalfEdge {
                        origin: last_point.clone(),
                        twin,
                        incident_face: face_id,
                        next: twin,
                        prev: twin,
                    },
                );
                self.half_edges.insert(
                    twin,
                    HalfEdge {
                        origin: point.clone(),
                        twin: id,
                        incident_face: other_face_id,
                        next: id,
                        prev: id,
                    },
                );
            }

            // Updating leftmost_vertex
            if let Some(cur_leftmost_vertex) = leftmost_vertex {
                if compare_points_x_then_y(
                    &last_point,
                    &self.half_edges[cur_leftmost_vertex].origin,
                )
                .is_lt()
                {
                    leftmost_vertex = Some(id);
                }
            } else {
                leftmost_vertex = Some(id);
            }

            last_edge = Some(id);
            last_point = point;
        }

        let first_edge = match first_edge {
            Some(v) => v,
            None => return,
        };

        let last_edge = match last_edge {
            Some(v) => v,
            None => return,
        };

        let leftmost_vertex = match leftmost_vertex {
            Some(v) => v,
            None => return,
        };

        // Connect the first and last edges.
        {
            let first_twin = self.half_edges[first_edge].twin;
            let last_twin = self.half_edges[last_edge].twin;

            self.half_edges[first_edge].prev = last_edge;
            self.half_edges[first_twin].next = last_twin;

            self.half_edges[last_edge].next = first_edge;
            self.half_edges[last_twin].prev = first_twin;
        }

        // Adding the faces.
        {
            let face_is_inner = {
                let edge = &self.half_edges[leftmost_vertex];
                let next_edge = &self.half_edges[edge.next];
                let prev_edge = &self.half_edges[edge.prev];
                !turns_right(&prev_edge.origin, &edge.origin, &next_edge.origin)
            };

            let mut inner_edge = first_edge;
            let mut outer_edge = self.half_edges[first_edge].twin;

            if !face_is_inner {
                core::mem::swap(&mut face_id, &mut other_face_id);
                core::mem::swap(&mut inner_edge, &mut outer_edge);
            }

            self.faces.insert(
                face_id,
                Face {
                    label,
                    outer_component: Some(inner_edge),
                    inner_components: vec![],
                },
            );

            // The outer face will be re-assigned to the unbounded face. This avoids having
            // many references to unbounded faces when merging complex geometries.
            let mut current_edge = outer_edge;
            while self.half_edges[current_edge].incident_face == other_face_id {
                self.half_edges[current_edge].incident_face = self.unbounded_face_id;
                current_edge = self.half_edges[current_edge].next;
            }

            /*
            self.faces.insert(
                other_face_id,
                Face {
                    label: F::default(),
                    outer_component: None,
                    inner_components: vec![outer_edge],
                },
            );
            */
        }
    }

    // TODO: Remove this.
    //
    // NOTE: Label will be the inner face if the polygon is built with
    // counter-clockwise vertices.
    fn add_first_edge(&mut self, start: Vector2f, end: Vector2f, label: F) -> EdgeId {
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
                origin: quantize2(start, self.scale),
                twin,
                incident_face: face_id,
                next: twin,
                prev: twin,
            },
        );
        self.half_edges.insert(
            twin,
            HalfEdge {
                origin: quantize2(end, self.scale),
                twin: id,
                incident_face: self.unbounded_face_id,
                next: id,
                prev: id,
            },
        );

        id
    }

    // TODO: Remove this.
    //
    // Helper for adding a line to a chain
    fn add_next_edge(&mut self, prev: EdgeId, next_point: Vector2f) -> EdgeId {
        let id = self.half_edges.unique_id();
        let twin = self.half_edges.unique_id();

        let prev_twin = self.half_edges[prev].twin;
        let last_point = self.destination(&self.half_edges[prev]);

        let incident_face = self.half_edges[prev].incident_face;
        let other_face = self.half_edges[prev].incident_face;

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
                origin: quantize2(next_point, self.scale),
                twin: id,
                incident_face: self.unbounded_face_id,
                next: prev_twin,
                prev: id,
            },
        );
        self.half_edges[prev_twin].prev = twin;

        id
    }

    pub fn add_close_edge(&mut self, last_edge: EdgeId, first_edge: EdgeId) -> EdgeId {
        let id = self.half_edges.unique_id();
        let twin = self.half_edges.unique_id();

        let last_origin = self.half_edges[last_edge].origin.clone();
        let last_dest = self.destination(&self.half_edges[last_edge]);
        let last_twin = self.half_edges[last_edge].twin;

        let first_origin = self.half_edges[first_edge].origin.clone();
        let first_twin = self.half_edges[first_edge].twin;

        let incident_face = self.half_edges[last_edge].incident_face;
        let other_face = self.half_edges[last_twin].incident_face;

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
                incident_face: other_face,
                next: last_twin,
                prev: first_twin,
            },
        );
        self.half_edges[first_twin].next = twin;
        self.half_edges[last_twin].prev = twin;

        id
    }

    fn destination(&self, edge: &HalfEdge) -> Vector2i64 {
        self.half_edges[edge.twin].origin.clone()
    }

    /// Combines self and other into one half edge data structure consisting of
    /// no overlapping edges/faces.
    ///
    /// TODO: How to deal with overlapping line segments (overlapping segments
    /// should intersect that their ).
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
                scale: self.scale,
            }
        };

        output.repair();
        output
    }

    /*
    - Main limitation is that the we can't handle making y-monotone faces if they have points that intersect at more than 2 edges.

    - Also don't currently correctly get rid of

    */

    /// Makes the current edge/face set completely 'valid'.
    ///
    /// This uses the 'MapOverlap' algorithm in the 'Computation Geometry -
    /// Algorithms and Applications' book Chapter 2.
    ///
    /// Valid means:
    /// - no intersecting/overlapping half edges or faces.
    /// - half-edges sorted with their faces to the left of them.
    pub fn repair(&mut self) {
        // Id of the edge immediately to the left of the origin vertex of each left (if
        // any).
        let mut edge_left_neighbors = HashMap::default();

        /// Extra original faces which are incident on an edge.
        /// (extras are introduced because some other edge )
        let mut edge_extra_faces = HashMap::default();

        self.remove_empty_edges();

        // Note that because we quantize the end points, we may end up introducing
        // additional intersections when quantizing.
        while self.remove_intersections() {}

        self.repair_edges(&mut edge_left_neighbors, &mut edge_extra_faces);

        // TODO: Bound this loop.
        loop {
            self.repair_faces(&edge_left_neighbors, &edge_extra_faces);

            // Remove extra internal edges that don't separate faces. Note that after this
            // step, faces may need to be re-computed as they may be split into two.
            if !self.merge_faces_impl(|a, b| false) {
                break;
            }

            edge_extra_faces.clear();
        }
    }

    /// Eliminate any edges with zero length.
    fn remove_empty_edges(&mut self) {
        let mut skip_ids = vec![];
        for (id, half_edge) in self.half_edges.iter() {
            if half_edge.origin == self.destination(half_edge) {
                skip_ids.push(*id);
            }
        }

        for id in skip_ids {
            let edge = self.half_edges.remove(&id).unwrap();

            // NOTE: 'edge.prev' may equal 'id' or may have been deleted in a prior
            // iteration.

            if let Some(prev) = self.half_edges.get_mut(&edge.prev) {
                prev.next = edge.next;
            }

            if let Some(next) = self.half_edges.get_mut(&edge.next) {
                next.prev = edge.prev;
            }
        }
    }

    fn edges_to_segments(&self) -> (Vec<LineSegment2<Rational>>, Vec<EdgeId>) {
        // Line segments extracted from each pair of half edges.
        let mut segments = vec![];

        // For each segment in 'segments' this is the id of the edge from which it was
        // derived.
        let mut segment_edge_ids = vec![];

        {
            for (id, half_edge) in self.half_edges.iter() {
                // Only index one half-edge per edge as they correspond to the same line
                // segment.
                if *id > half_edge.twin {
                    continue;
                }

                segments.push(LineSegment2 {
                    start: half_edge.origin.cast::<Rational>(),
                    end: self.destination(half_edge).cast::<Rational>(),
                });
                segment_edge_ids.push(*id);
            }
        }

        (segments, segment_edge_ids)
    }

    /// Splits any edges which intersect with other edges at a non-endpoint.
    ///
    /// Returns whether or not any changes were made.
    fn remove_intersections(&mut self) -> bool {
        // Whether or not we performed any splitting.
        let mut split = false;

        let (segments, mut segment_edge_ids) = self.edges_to_segments();

        let intersections = LineSegment2::intersections(&segments);

        for intersection in intersections {
            // TODO: Take all consecutive intersections that quantize to the same point
            // (though some may be way further down in the list so we need to quantize
            // first, then re-sort the list, and then do stuff).

            // TODO: If <= the previous intersection, round up in 'X' and take all other
            // similar intersections.
            let intersection_point_quantized = intersection.point.round().cast::<i64>();

            for segment_idx in intersection.segments.iter().cloned() {
                let edge_id = segment_edge_ids[segment_idx];
                let edge = self.half_edges[edge_id].clone();
                let edge_twin = self.half_edges[edge.twin].clone();
                let edge_dest = edge_twin.origin;

                // TODO: If our threshold is larger than one quantized unit, this must use
                // in-exact comparison.
                let origin_equal = edge.origin == intersection_point_quantized;
                let dest_equal = edge_dest == intersection_point_quantized;

                if !origin_equal && !dest_equal {
                    split = true;

                    // 'edge_id': origin -> intersection
                    // 'id1': intersection -> edge_dest
                    let id1 = self.half_edges.unique_id();
                    self.half_edges.insert(
                        id1,
                        HalfEdge {
                            origin: intersection_point_quantized.clone(),
                            twin: edge.twin,
                            incident_face: edge.incident_face.clone(),
                            next: edge.next,
                            prev: edge_id,
                        },
                    );
                    self.half_edges[edge_id].next = id1;
                    self.half_edges[edge.twin].twin = id1;

                    // 'edge.twin': edge_dest -> intersection
                    // 'id2': intersection -> edge.origin
                    let id2 = self.half_edges.unique_id();

                    self.half_edges.insert(
                        id2,
                        HalfEdge {
                            origin: intersection_point_quantized.clone(),
                            twin: edge_id,
                            incident_face: edge_twin.incident_face.clone(),
                            next: edge_twin.next,
                            prev: edge.twin,
                        },
                    );
                    self.half_edges[edge.twin].next = id2;
                    self.half_edges[edge_id].twin = id2;

                    // Update the segment to correct to the portion of the original segment which
                    // still remains to be matched below (/ to the right of) the sweep line.
                    segment_edge_ids[segment_idx] =
                        if compare_points_i64(&edge.origin, &edge_dest).is_gt() {
                            edge_id
                        } else {
                            edge.twin
                        };

                    split = true;
                }
            }
        }

        split
    }

    /// Repairing of just the half_edges. By the edge of this function:
    /// - Half-edges will be sorted around each intersection point.
    /// - No edges should be overlapping.
    ///
    /// Pre-requisite: There are no partially overlapping edges in the data
    /// structure.
    ///
    /// NOTE: This assumes that there are no partially overlapping edges (this
    /// will clean up completely overlapping edges, but not split partially
    /// overlapping ones). Partial overlaps are hard to handle here since
    /// splitting and quantizing the split point will change the angles of
    /// individual edges. The below algorithm relies on stable angles of edges
    /// around intersection points to ensure that we don't need to backtrack and
    /// re-sort intersection points.
    fn repair_edges(
        &mut self,
        edge_left_neighbors: &mut HashMap<EdgeId, EdgeId, FastHasherBuilder>,
        edge_extra_faces: &mut HashMap<EdgeId, Vec<FaceId>, FastHasherBuilder>,
    ) {
        // TODO: When an intersection only has two segments (after deleting redundant
        // ones) and their angles are exactly opposite each other, merge the lines
        // together.

        // TODO: Delete any non-closed cycles (anything that loops to a twin edge)

        let (segments, mut segment_edge_ids) = self.edges_to_segments();

        // Segments that we are deleting since they are redundant with another identical
        // edge.
        //
        // - Deletion will prevent the segment's involvement in future intersection
        //   point fixing rounds.
        // - Merging of faces is also recorded in the 'edge_extra_faces' output argument
        //   of this function.
        // - After all intersection points have been fixed (so no longer contain these
        //   segments), we can delete these from self.half_edges.
        let mut deleted_segments = HashMap::<_, _, FastHasherBuilder>::default();

        // TODO: This could be a streaming iterator.
        let intersections = LineSegment2::intersections(&segments);

        for intersection in intersections {
            let intersection_point = intersection.point.cast::<i64>();

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

                segment: usize,
            }

            // List of all edges converging at the intersection point.
            let mut intersecting_edges = vec![];

            for segment_idx in intersection.segments.iter().cloned() {
                if deleted_segments.contains_key(&segment_idx) {
                    continue;
                }

                let edge_id = segment_edge_ids[segment_idx];
                let edge = self.half_edges[edge_id].clone();
                let edge_dest = self.destination(&edge);

                // TODO: If our threshold is larger than one quantized unit, this must use
                // in-exact comparison.
                let origin_equal = edge.origin == intersection_point;
                let dest_equal = edge_dest == intersection_point;

                if origin_equal {
                    assert!(!dest_equal);

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
                        segment: segment_idx,
                        angle: 0.into(), // Computed later
                    });
                } else if dest_equal {
                    assert!(!origin_equal);

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
                        segment: segment_idx,
                        angle: 0.into(), // Computed later
                    });
                } else {
                    panic!();
                }
            }

            // Sort edges by ascending clockwise angle
            for edge in &mut intersecting_edges {
                let dir = edge.point.cast::<Rational>() - &intersection.point;
                edge.angle = dir.pseudo_angle();
            }
            intersecting_edges.sort_by(|a, b| {
                let angle_ordering = b.angle.cmp(&a.angle);
                if angle_ordering.is_ne() {
                    return angle_ordering;
                }

                let a_id = core::cmp::min(a.inward_id, a.outward_id);
                let b_id = core::cmp::min(b.inward_id, b.outward_id);
                a_id.cmp(&b_id)
            });

            // Remove overlapping edges.
            intersecting_edges.dedup_by(|next_edge, edge| {
                if edge.angle == next_edge.angle {
                    // Since we removed all non-endpoint intersections, this should always be true.
                    assert_eq!(edge.point, next_edge.point);

                    deleted_segments.insert(next_edge.segment, edge.segment);

                    // NOTE: We can only do this now since the two edges completely overlap. If they
                    // only partially overlapped, then we can't assign the labels of the longer one
                    // to the shorter one.
                    edge_extra_faces
                        .entry(edge.inward_id)
                        .or_default()
                        .push(next_edge.inward_face);
                    edge_extra_faces
                        .entry(edge.outward_id)
                        .or_default()
                        .push(next_edge.outward_face);

                    true
                } else {
                    // TODO: Assert not deleted (need at least one edge with each angle).

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

                if let Some(mut left_neighbor) = intersection.left_neighbor.clone() {
                    if let Some(deduped_segment) = deleted_segments.get(&left_neighbor) {
                        left_neighbor = *deduped_segment;
                    }

                    edge_left_neighbors.insert(edge.outward_id, segment_edge_ids[left_neighbor]);
                }
            }
        }

        // Perform the actual deletions.
        for idx in deleted_segments.keys() {
            let id = segment_edge_ids[*idx];
            let edge = self.half_edges.remove(&id).unwrap();
            let twin = self.half_edges.remove(&edge.twin).unwrap();
        }
    }

    fn repair_faces(
        &mut self,
        edge_left_neighbors: &HashMap<EdgeId, EdgeId, FastHasherBuilder>,
        edge_extra_faces: &HashMap<EdgeId, Vec<FaceId>, FastHasherBuilder>,
    ) {
        #[derive(Debug)]
        struct Boundary {
            edges: Vec<EdgeId>,
            is_inner: bool,
            leftmost_vertex: EdgeId,

            /// Index of the boundary which lies to the left of the leftmost
            /// vertex of this boundary.
            parent: Option<usize>,

            // Indices of other boundaries which are children of this boundary.
            children: Vec<usize>,

            /// Ids of all old faces which contain this boundary.
            label_faces: HashSet<FaceId, FastHasherBuilder>,
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
        let mut edge_to_boundary_index = HashMap::<EdgeId, usize, FastHasherBuilder>::default();

        // Find all boundary cycles by traversing all the edges.
        // (parent/child relationships not yet populated)
        for (edge_id, edge) in self.half_edges.iter() {
            if edge_to_boundary_index.contains_key(edge_id) {
                continue;
            }

            let mut edges = vec![];

            // Leftmost (lowest if multiple) vertex of the boundary.
            let mut leftmost_vertex = *edge_id;

            {
                let mut current_id = *edge_id;
                while !edge_to_boundary_index.contains_key(&current_id) {
                    edges.push(current_id);
                    edge_to_boundary_index.insert(current_id, boundaries.len());

                    let edge = &self.half_edges[current_id];

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
                is_inner,
                leftmost_vertex,
                // To be populated in the next loop.
                parent: None,
                children: vec![],
                label_faces: HashSet::default(),
            });
        }

        // Link all inner boundaries to the boundary immediately to the left of them.
        // (populating the parent/child fields in all the boundaries).
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

        let mut have_labels = false;
        for face in self.faces.values() {
            if face.label != F::default() {
                have_labels = true;
                break;
            }
        }

        // To figure out which labels to assign to each new boundary we trace a
        // horizontal scanline from left to right of a point that is within the
        // boundary.
        //
        // - Given we have the leftmost vertex (x,y) of the boundary, there is a point
        //   '(x + a, y + b)' that is inside of the boundary.
        //  - 'a' is an infinitely small non-zero positive value.
        //  - 'b' is an infinitely small non-zero positive OR negative value.
        //    - if either edge connected to (x,y) goes up, then 'b' is positive.
        //    - else, both edges go down (or one is horizontal) so 'b' is negative.
        // - In all cases, all edges that interesect with a horizontal scanline at 'y +
        //   b' must pass through 'y' so we just need to we can do a scanline sweep at
        //   that point.
        // - Intersection points are sorted by x intersect. For points with the same x
        //   intersect, they are sorted by angle above or below (depending on the sign
        //   of 'b') the scanline from left and right.
        // - Edges that intersect at '(x,y)' must be <= the two edges connecting to
        //   (x,y) in the current boundary to be included in the scan.
        //
        // Note that while the original face edge cycles have already been destroyed,
        // the individual edges still contain accurate incident face metadata.
        //
        // TODO: This is currently very slow and needs to be re-implemented with a plane
        // sweep / LineSegment::intersections.
        if have_labels {
            // TODO: Pre-filter horizontal segments.
            let (mut segments, segment_edge_ids) = self.edges_to_segments();

            // Normalize so that the upper segment is the start.
            for segment in &mut segments {
                if segment.start.y() < segment.end.y() {
                    core::mem::swap(&mut segment.start, &mut segment.end);
                }
            }

            let mut intersections = vec![];

            for boundary in &mut boundaries {
                if boundary.is_inner {
                    continue;
                }

                let mut boundary_vertex_id = boundary.leftmost_vertex;

                let boundary_vertex = self.half_edges[boundary_vertex_id].origin.clone();
                let boundary_vertex_below = self.destination(&self.half_edges[boundary_vertex_id]);
                let boundary_vertex_above = self.half_edges
                    [self.half_edges[boundary_vertex_id].prev]
                    .origin
                    .clone();

                assert!(boundary_vertex != boundary_vertex_above);
                assert!(boundary_vertex_below != boundary_vertex_above);

                // TODO: Should I check both points?
                let face_below_boundary_vertex = boundary_vertex_below.y() < boundary_vertex.y();
                let face_above_boundary_vertex = boundary_vertex_above.y() > boundary_vertex.y();

                // This will only be false if we are looking at a horizontal line.
                // TODO: This may happen for self intersecting faces.
                assert!(face_below_boundary_vertex || face_above_boundary_vertex);

                let boundary_angle_below = (boundary_vertex_below.cast::<Rational>()
                    - boundary_vertex.cast::<Rational>())
                .pseudo_angle();

                let boundary_angle_above = (boundary_vertex_above.cast::<Rational>()
                    - boundary_vertex.cast::<Rational>())
                .pseudo_angle();

                // let mut intersections = vec![];
                intersections.clear();

                let x = Rational::from(boundary_vertex.x());
                let y = Rational::from(boundary_vertex.y());
                for (i, segment) in segments.iter().enumerate() {
                    // Filtering speed performance optimization.
                    if segment.start.y() < y || segment.end.y() > y {
                        continue;
                    }

                    // NOTE: This should filter out all segments that don't intersect with the
                    // scan line at 'y' and all horizontal lines.
                    let x_i = match segment.evaluate_at_y(y) {
                        Some(v) => v,
                        None => continue,
                    };

                    if x_i > x {
                        continue;
                    }

                    let angle = {
                        if face_above_boundary_vertex {
                            // Skip if the segment doesn't go higher than y.
                            if segment.start.y() == y {
                                continue;
                            }

                            // TODO: This angle can be cached for a segment.
                            let angle = (&segment.start - &segment.end).pseudo_angle();

                            if x_i == x && angle < boundary_angle_above {
                                continue;
                            }

                            -angle
                        } else {
                            // Skip if the segment doesn't go lower than y.
                            if segment.end.y() == y {
                                continue;
                            }

                            // TODO: This angle can be cached for a segment.
                            let angle = (&segment.end - &segment.start).pseudo_angle();

                            if x_i == x && angle > boundary_angle_below {
                                continue;
                            }

                            angle
                        }
                    };

                    intersections.push((x_i, angle, segment_edge_ids[i]));
                }

                intersections.sort();

                for (_, _, mut edge_id) in &intersections {
                    let mut twin_id = self.half_edges[edge_id].twin;

                    // Normalize so that edge_id is the one pointing down
                    let mut a = self.half_edges[edge_id].origin.clone();
                    let mut b = self.half_edges[twin_id].origin.clone();
                    assert!(a.y() != b.y());
                    if a.y() < b.y() {
                        core::mem::swap(&mut edge_id, &mut twin_id);
                        core::mem::swap(&mut a, &mut b);
                    }

                    // Include/exclude stuff now.

                    boundary
                        .label_faces
                        .remove(&self.half_edges[twin_id].incident_face);
                    if let Some(faces) = edge_extra_faces.get(&twin_id) {
                        for face in faces {
                            boundary.label_faces.remove(face);
                        }
                    }

                    boundary
                        .label_faces
                        .insert(self.half_edges[edge_id].incident_face);
                    if let Some(faces) = edge_extra_faces.get(&edge_id) {
                        boundary.label_faces.extend(faces.iter().cloned());
                    }
                }
            }
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

                // TODO: Deduplicate this logic with the new face case.

                unbounded_face
                    .inner_components
                    .push(boundary.leftmost_vertex);

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
            } else {
                // Form a new face.

                let face_id = faces.unique_id();

                let mut label = F::default();

                for id in &boundary.label_faces {
                    label = label.union(&self.faces[*id].label);
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

    /// Returns whether or not any edges were modified.
    fn merge_faces_impl<M: Fn(&Face<F>, &Face<F>) -> bool>(&mut self, can_merge: M) -> bool {
        let mut edges_to_remove = vec![];

        for (edge_id, edge) in self.half_edges.iter() {
            if *edge_id > edge.twin {
                continue;
            }

            if edge.incident_face == self.half_edges[edge.twin].incident_face
                || can_merge(
                    &self.faces[edge.incident_face],
                    &self.faces[self.half_edges[edge.twin].incident_face],
                )
            {
                edges_to_remove.push(*edge_id);
            }
        }

        let some_removed = !edges_to_remove.is_empty();
        for edge_id in edges_to_remove {
            let edge = self.half_edges.remove(&edge_id).unwrap();
            let twin = self.half_edges.remove(&edge.twin).unwrap();

            if edge.next != edge.twin {
                self.half_edges[edge.next].prev = twin.prev;
                self.half_edges[twin.prev].next = edge.next;
            }

            if twin.next != edge_id {
                self.half_edges[twin.next].prev = edge.prev;
                self.half_edges[edge.prev].next = twin.next;
            }
        }

        some_removed
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

        let mut line_segments: Vec<LineSegment2<Rational>> = vec![];
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
                    start: edge.origin.cast(),
                    end: self.destination(edge).cast(),
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

        let mut lowest_interior_points = HashMap::<_, _, FastHasherBuilder>::default();

        // NOTE: All of these intersections will occur at existing line endpoints since
        // we assume that self has already been repaired.
        let intersections = LineSegment2::intersections(&line_segments);

        // Iterate over vertices in the face (as all our faces should be closed, this
        // corresponds to each intersection point too).
        //
        // TODO: Execute that at the same time as the repair() process.
        for intersection in intersections {
            if intersection.segments.len() != 2 {
                println!("{:#?}", intersection);
            }

            // Always true as we are only considering a single face at a time.
            //
            // TODO: Having more than two segments will happen when a face loops back to its
            // start (e.g. cut donut)
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
            assert!(edge.origin.cast() == intersection.point);

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
                    lowest_interior_points.insert(edge_id, (edge_id, VertexType::Split));
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

impl<F: FaceLabel + PartialEq> HalfEdgeStruct<F> {
    pub fn merge_faces(&mut self) {
        self.merge_faces_impl(|a, b| a.label == b.label);
        self.repair();
    }
}

impl<F> HalfEdgeStruct<F> {
    /// NOTE: The unbounded face will always have a default valued label.
    pub fn map_labels<G: Default, T: Fn(&F) -> G>(&self, transform: T) -> HalfEdgeStruct<G> {
        let mut faces = EntityStorage::new();
        faces.next_id = self.faces.next_id;
        for (id, face) in self.faces.iter() {
            let label = {
                if *id == self.unbounded_face_id {
                    G::default()
                } else {
                    transform(&face.label)
                }
            };

            faces.insert(
                id.clone(),
                Face {
                    label,
                    outer_component: face.outer_component.clone(),
                    inner_components: face.inner_components.clone(),
                },
            );
        }

        HalfEdgeStruct {
            half_edges: self.half_edges.clone(),
            faces,
            unbounded_face_id: self.unbounded_face_id,
            scale: self.scale,
        }
    }
}

pub struct FacesIterator<'a, F> {
    inst: &'a HalfEdgeStruct<F>,
    faces: EntityStorageIter<'a, FaceId, Face<F>>,
}

impl<'a, F> Iterator for FacesIterator<'a, F> {
    type Item = FaceReference<'a, F>;

    fn next(&mut self) -> Option<Self::Item> {
        self.faces.next().map(|(id, face)| FaceReference {
            id: *id,
            inst: self.inst,
            face,
        })
    }
}

pub struct FaceReference<'a, F> {
    inst: &'a HalfEdgeStruct<F>,
    id: FaceId,
    face: &'a Face<F>,
}

impl<'a, F> FaceReference<'a, F> {
    pub fn id(&self) -> FaceId {
        self.id
    }

    pub fn label(&self) -> &F {
        &self.face.label
    }

    pub fn is_unbounded_face(&self) -> bool {
        self.id == self.inst.unbounded_face_id
    }

    pub fn outer_component(&self) -> Option<ComponentReference<'a, F>> {
        self.face
            .outer_component
            .map(|start_id| ComponentReference {
                inst: self.inst,
                start_id,
            })
    }

    pub fn inner_components<'b>(&'b self) -> impl Iterator<Item = ComponentReference<'a, F>> + 'b {
        self.face
            .inner_components
            .iter()
            .cloned()
            .map(move |start_id| ComponentReference {
                inst: self.inst,
                start_id,
            })
    }
}

pub struct ComponentReference<'a, F> {
    inst: &'a HalfEdgeStruct<F>,
    start_id: EdgeId,
}

impl<'a, F> ComponentReference<'a, F> {
    pub fn points(&self) -> Vec<Vector2f> {
        let mut current_edge_id = self.start_id;

        let mut out = vec![];

        bounded_loop(self.inst.half_edges.len() + 1, || {
            let current_edge = &self.inst.half_edges[current_edge_id];
            out.push(dequantize2(current_edge.origin.clone(), self.inst.scale));
            current_edge_id = current_edge.next;

            Ok(if current_edge_id == self.start_id {
                Loop::Break
            } else {
                Loop::Continue
            })
        })
        .unwrap();

        out
    }

    pub fn start_id(&self) -> EdgeId {
        self.start_id
    }

    pub fn start_edge(&self) -> HalfEdgeReference<'a, F> {
        HalfEdgeReference {
            inst: self.inst,
            id: self.start_id
        }
    }
}

pub struct HalfEdgeReference<'a, F> {
    inst: &'a HalfEdgeStruct<F>,
    id: EdgeId,
}

impl<'a, F> HalfEdgeReference<'a, F> {

    pub fn id(&self) -> EdgeId {
        self.id
    } 

    pub fn incident_face(&self) -> FaceReference<'a, F> {
        let this = &self.inst.half_edges[self.id];

        FaceReference {
            inst: self.inst,
            id: this.incident_face,
            face: &self.inst.faces[this.incident_face]
        }
    }

    pub fn origin(&self) -> Vector2f {
        let this = &self.inst.half_edges[self.id];
        dequantize2(this.origin.clone(), self.inst.scale)
    }

    pub fn next(&self) -> HalfEdgeReference<'a, F> {
        let this = &self.inst.half_edges[self.id];

        HalfEdgeReference {
            inst: self.inst,
            id: this.next,
        }
    }

    pub fn twin(&self) -> HalfEdgeReference<'a, F> {
        let this = &self.inst.half_edges[self.id];

        HalfEdgeReference {
            inst: self.inst,
            id: this.twin,
        }
    }
}



// pub struct Faces

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
            assert!(seen_ids.contains(edge_id), "Missing {:?}", edge_id);

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
            boundary.push(dequantize2(current_edge.origin.clone(), data.scale));
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
                origin: quantize2(vec2f(0., 0.), DEFAULT_SCALE),
                twin: e2,
                next: e2,
                prev: e2,
                incident_face: data.unbounded_face_id,
            },
        );
        data.half_edges.insert(
            e2,
            HalfEdge {
                origin: quantize2(vec2f(10., 10.), DEFAULT_SCALE),
                twin: e1,
                next: e1,
                prev: e1,
                incident_face: data.unbounded_face_id,
            },
        );
        data.half_edges.insert(
            e3,
            HalfEdge {
                origin: quantize2(vec2f(10., 0.), DEFAULT_SCALE),
                twin: e4,
                next: e4,
                prev: e4,
                incident_face: data.unbounded_face_id,
            },
        );
        data.half_edges.insert(
            e4,
            HalfEdge {
                origin: quantize2(vec2f(0., 10.), DEFAULT_SCALE),
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

        data.add_face(
            label("A"),
            [
                vec2f(0., 0.),
                vec2f(20., 0.),
                vec2f(20., 20.),
                vec2f(0., 20.),
            ]
            .into_iter()
            .cloned(),
        );

        data.add_face(
            label("B"),
            [
                vec2f(5., 5.),
                vec2f(15., 5.),
                vec2f(15., 15.),
                vec2f(5., 15.),
            ]
            .into_iter()
            .cloned(),
        );

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
        //          | B  |
        // ------   |    |
        // | A  |   ------
        // |    |
        // ------

        let mut data = HalfEdgeStruct::new();

        data.add_face(
            label("A"),
            [
                vec2f(0., 0.),
                vec2f(10., 0.),
                vec2f(10., 10.),
                vec2f(0., 10.),
            ]
            .iter()
            .cloned(),
        );

        data.add_face(
            label("B"),
            [
                vec2f(15., 5.),
                vec2f(25., 5.),
                vec2f(25., 15.),
                vec2f(15., 15.),
            ]
            .iter()
            .cloned(),
        );

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
    fn triangle_on_same_triangle() {
        let mut data = HalfEdgeStruct::new();
        {
            let a = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("A"));
            let b = data.add_next_edge(a, vec2f(5., 5.));
            data.add_close_edge(b, a);
        }

        let mut data2 = HalfEdgeStruct::new();
        {
            let a = data2.add_first_edge(vec2f(0., 0.), vec2f(10., 0.), label("B"));
            let b = data2.add_next_edge(a, vec2f(5., 5.));
            data2.add_close_edge(b, a);
        }

        let data3 = data.overlap(&data2);

        let boundaries = FaceDebug::get_all(&data3);

        assert_that(
            &boundaries,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(5.0, 5.0),
                        vec2f(10.0, 0.0),
                    ]],
                }),
                eq(FaceDebug {
                    label: labels(&["A", "B"]),
                    outer_component: Some(vec![vec2f(0.0, 0.0), vec2f(10.0, 0.0), vec2f(5.0, 5.0)]),
                    inner_components: vec![],
                }),
            ]),
        );
    }

    fn add_face<F: FaceLabel>(inst: &mut HalfEdgeStruct<F>, label: F, points: &[(f32, f32)]) {
        assert!(points.len() >= 3);

        let first_edge = inst.add_first_edge(
            vec2f(points[0].0, points[0].1),
            vec2f(points[1].0, points[1].1),
            label,
        );

        let mut last_edge = first_edge;

        for i in 2..points.len() {
            last_edge = inst.add_next_edge(last_edge, vec2f(points[i].0, points[i].1));
        }

        inst.add_close_edge(last_edge, first_edge);
    }

    #[test]
    fn square_on_bigger_square() {
        /*
        ------------------
        |        |    |   |
        |  ABC   |    |   |
        |        |    |   |
        ----------    |   |
        |     AB      |   |
        |-------------|   |
        |        A        |
        ------------------
        */

        let mut data = HalfEdgeStruct::new();

        // Outer
        add_face(
            &mut data,
            label("A"),
            &[(0.0, 0.0), (3.0, 0.0), (3.0, 3.0), (0.0, 3.0)],
        );

        // Middle
        add_face(
            &mut data,
            label("B"),
            &[(0.0, 1.0), (2.0, 1.0), (2.0, 3.0), (0.0, 3.0)],
        );

        // Inner (top-left)
        add_face(
            &mut data,
            label("C"),
            &[(0.0, 2.0), (1.0, 2.0), (1.0, 3.0), (0.0, 3.0)],
        );

        data.repair();

        let boundaries = FaceDebug::get_all(&data);

        assert_that(
            &boundaries,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 1.0),
                        vec2f(0.0, 2.0),
                        vec2f(0.0, 3.0),
                        vec2f(1.0, 3.0),
                        vec2f(2.0, 3.0),
                        vec2f(3.0, 3.0),
                        vec2f(3.0, 0.0),
                    ]],
                }),
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(3.0, 0.0),
                        vec2f(3.0, 3.0),
                        vec2f(2.0, 3.0),
                        vec2f(2.0, 1.0),
                        vec2f(0.0, 1.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: labels(&["A", "B"]),
                    outer_component: Some(vec![
                        vec2f(0.0, 1.0),
                        vec2f(2.0, 1.0),
                        vec2f(2.0, 3.0),
                        vec2f(1.0, 3.0),
                        vec2f(1.0, 2.0),
                        vec2f(0.0, 2.0),
                    ]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: labels(&["A", "B", "C"]),
                    outer_component: Some(vec![
                        vec2f(0.0, 2.0),
                        vec2f(1.0, 2.0),
                        vec2f(1.0, 3.0),
                        vec2f(0.0, 3.0),
                    ]),
                    inner_components: vec![],
                }),
            ]),
        );
    }

    #[test]
    fn single_square() {
        let mut half_edges = HalfEdgeStruct::<bool>::new();

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

        half_edges.repair();

        let faces = FaceDebug::get_all(&half_edges);

        assert_that(
            &faces,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: false,
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 1.0),
                        vec2f(1.0, 1.0),
                        vec2f(1.0, 0.0),
                    ]],
                }),
                eq(FaceDebug {
                    label: true,
                    outer_component: Some(vec![
                        vec2f(0.0, 0.0),
                        vec2f(1.0, 0.0),
                        vec2f(1.0, 1.0),
                        vec2f(0.0, 1.0),
                    ]),
                    inner_components: vec![],
                }),
            ]),
        );
    }

    #[test]
    fn square_of_triangles_test() {
        let mut half_edges = HalfEdgeStruct::new();

        half_edges.add_face(
            label("A"),
            [vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0)]
                .iter()
                .cloned(),
        );

        half_edges.add_face(
            label("B"),
            [vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0)]
                .iter()
                .cloned(),
        );

        half_edges.repair();

        let faces = FaceDebug::get_all(&half_edges);

        assert_that(
            &faces,
            unordered_elements_are(&[
                eq(FaceDebug {
                    label: labels(&[]),
                    outer_component: None,
                    inner_components: vec![vec![
                        vec2f(0.0, 0.0),
                        vec2f(0.0, 1.0),
                        vec2f(1.0, 1.0),
                        vec2f(1.0, 0.0),
                    ]],
                }),
                eq(FaceDebug {
                    label: label("A"),
                    outer_component: Some(vec![vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0)]),
                    inner_components: vec![],
                }),
                eq(FaceDebug {
                    label: label("B"),
                    outer_component: Some(vec![vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0)]),
                    inner_components: vec![],
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

        data.add_face(labels(&[]), points.iter().cloned());

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
