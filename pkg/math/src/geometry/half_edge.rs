use alloc::vec::Vec;
use core::f32::consts::PI;
use core::ops::Add;
use std::collections::HashMap;
use std::collections::HashSet;

use crate::geometry::convex_hull::turns_right;
use crate::geometry::line_segment::{compare_points, compare_points_x_then_y, LineSegment2f};
use crate::matrix::Vector2f;

/*
The face associated with each edge lies to the left of the ddge.

Half edges stored in counterclockwise order

Hole boundaries have edges sorted in clockwise order.

TODOs:
- Need resilience to having multiple edges which use duplicate start/end points.

*/

#[derive(Debug)]
pub struct HalfEdgeStruct {
    half_edges: HashMap<EdgeId, HalfEdge>,
    next_edge_id: EdgeId,
    faces: HashMap<FaceId, Face>,
    next_face_id: FaceId,
}

#[derive(Debug, Clone)]
struct Face {
    /// Some edge on the outer most boundary of this face.
    /// If none, then this is the unbounded face surrounding all polygons.
    outer_component: Option<EdgeId>,

    /// Some edge of each face inside the outer component (holes).
    inner_components: Vec<EdgeId>,
}

enum BoundaryType {
    Inner,
    Outer,
}

#[derive(Clone, Debug)]
struct HalfEdge {
    origin: Vector2f,
    twin: EdgeId,

    incident_face: FaceId,
    // incident_face_component: BoundaryType,
    next: EdgeId,
    prev: EdgeId,
}

impl HalfEdgeStruct {
    pub fn new() -> Self {
        // let mut faces =

        Self {
            half_edges: HashMap::new(),
            next_edge_id: EdgeId(0),
            faces: HashMap::new(),
            next_face_id: FaceId(0),
        }
    }

    fn new_edge_id(&mut self) -> EdgeId {
        let id = self.next_edge_id;
        self.next_edge_id = id + EdgeId(1);
        id
    }

    fn add_first_edge(&mut self, start: Vector2f, end: Vector2f) -> EdgeId {
        let id = self.new_edge_id();
        let twin = self.new_edge_id();

        self.half_edges.insert(
            id,
            HalfEdge {
                origin: start,
                twin,
                incident_face: FaceId(0), // TODO
                next: twin,
                prev: twin,
            },
        );
        self.half_edges.insert(
            twin,
            HalfEdge {
                origin: end,
                twin: id,
                incident_face: FaceId(0), // TODO
                next: id,
                prev: id,
            },
        );

        id
    }

    // Helper for adding a line to a chain
    fn add_next_edge(&mut self, prev: EdgeId, next_point: Vector2f) -> EdgeId {
        let id = self.new_edge_id();
        let twin = self.new_edge_id();

        let prev_twin = self.half_edges[&prev].twin;
        let last_point = self.destination(&self.half_edges[&prev]);

        self.half_edges.insert(
            id,
            HalfEdge {
                origin: last_point,
                twin,
                incident_face: FaceId(0), // TODO
                next: twin,
                prev,
            },
        );
        self.half_edges.get_mut(&prev).unwrap().next = id;

        self.half_edges.insert(
            twin,
            HalfEdge {
                origin: next_point,
                twin: id,
                incident_face: FaceId(0), // TODO
                next: prev_twin,
                prev: id,
            },
        );
        self.half_edges.get_mut(&prev_twin).unwrap().prev = twin;

        id
    }

    fn add_close_edge(&mut self, last_edge: EdgeId, first_edge: EdgeId) {
        let id = self.new_edge_id();
        let twin = self.new_edge_id();

        let last_origin = self.half_edges[&last_edge].origin.clone();
        let last_dest = self.destination(&self.half_edges[&last_edge]);
        let last_twin = self.half_edges[&last_edge].twin;

        let first_origin = self.half_edges[&first_edge].origin.clone();
        let first_twin = self.half_edges[&first_edge].twin;

        self.half_edges.insert(
            id,
            HalfEdge {
                origin: last_dest,
                twin: twin,
                incident_face: FaceId(0), // TODO
                next: first_edge,
                prev: last_edge,
            },
        );
        self.half_edges.get_mut(&last_edge).unwrap().next = id;
        self.half_edges.get_mut(&first_edge).unwrap().prev = id;

        self.half_edges.insert(
            twin,
            HalfEdge {
                origin: first_origin,
                twin: id,
                incident_face: FaceId(0), // TODO
                next: last_twin,
                prev: first_twin,
            },
        );
        self.half_edges.get_mut(&first_twin).unwrap().next = twin;
        self.half_edges.get_mut(&last_twin).unwrap().prev = twin;
    }

    fn destination(&self, edge: &HalfEdge) -> Vector2f {
        self.half_edges[&edge.twin].origin.clone()
    }

    // TODO: How to deal with overlapping line segments (overlapping segments should
    // intersect that their ).
    pub fn overlap(&self, other: &HalfEdgeStruct) -> Self {
        // First concatenate the edge sets.
        // Ids of the second set at shifted to avoid overlaps.
        let mut output = {
            let mut half_edges = self.half_edges.clone();
            for (id, edge) in other.half_edges.iter() {
                half_edges.insert(
                    *id + self.next_edge_id,
                    HalfEdge {
                        origin: edge.origin.clone(),
                        incident_face: edge.incident_face + self.next_face_id,
                        twin: edge.twin + self.next_edge_id,
                        next: edge.next + self.next_edge_id,
                        prev: edge.prev + self.next_edge_id,
                    },
                );
            }

            let next_edge_id = self.next_edge_id + other.next_edge_id;

            let mut faces = self.faces.clone();
            for (id, face) in other.faces.iter() {
                faces.insert(
                    *id + self.next_face_id,
                    Face {
                        outer_component: face
                            .outer_component
                            .clone()
                            .map(|edge_id| edge_id + self.next_edge_id),
                        inner_components: face
                            .inner_components
                            .iter()
                            .cloned()
                            .map(|edge_id| edge_id + self.next_edge_id)
                            .collect(),
                    },
                );
            }

            let next_face_id = self.next_face_id + other.next_face_id;

            Self {
                half_edges,
                next_edge_id,
                faces,
                next_face_id,
            }
        };

        output.repair();
        output
    }

    /// Makes the current edge/face set completely 'valid'. In particular, we
    /// want the structure to contain no intersecting/overlapping half edges or
    /// faces.
    pub fn repair(&mut self) {
        let mut segments = vec![];

        // For each segment in 'segments' this is the id of the edge from which it was
        // derived.
        let mut segment_edge_ids = vec![];

        {
            for (id, half_edge) in self.half_edges.iter() {
                // Only index one half-edge per edge as they correct to the same line segment.
                if id.0 > half_edge.twin.0 {
                    continue;
                }

                segments.push(LineSegment2f {
                    start: half_edge.origin.clone(),
                    end: self.destination(half_edge),
                });
                segment_edge_ids.push(*id);
            }
        }

        // Id of the edge immediately to the left of the origin vertex of each left (if
        // any).
        let mut edge_left_neighbors = HashMap::new();

        let intersections = LineSegment2f::intersections(&segments);

        for intersection in intersections {
            // TODO: Stop early if the intersection point is strictly on endpoints of
            // existing edges.

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

                // Id of the edge directed away
                outward_id: EdgeId,

                outward_next: EdgeId,

                outward_face: FaceId,

                // Other endpoint of this edge aside of the intersection.point.
                point: Vector2f,
            }

            // List of all edges converging at the intersection point.
            let mut intersecting_edges = vec![];

            for segment_idx in intersection.segments.iter().cloned() {
                let edge_id = segment_edge_ids[segment_idx];
                let edge = &self.half_edges[&edge_id];
                let edge_dest = self.destination(edge);

                {
                    let segment = LineSegment2f {
                        start: edge.origin.clone(),
                        end: edge_dest.clone(),
                    };

                    assert!(segment.contains(&intersection.point));
                }

                let origin_equal = compare_points(&edge.origin, &intersection.point).is_eq();
                let dest_equal = compare_points(&edge_dest, &intersection.point).is_eq();

                if compare_points(&edge.origin, &intersection.point).is_eq() {
                    assert!(!dest_equal);

                    // The current edge is outward.
                    // self.half_edges[&edge.twin].next MUST also be in the current intersection.
                    intersecting_edges.push(PartialEdge {
                        inward_id: edge.twin,
                        inward_prev: self.half_edges[&edge.twin].prev,
                        inward_face: self.half_edges[&edge.twin].incident_face,
                        outward_id: edge_id,
                        outward_next: edge.next,
                        outward_face: edge.incident_face,
                        point: edge_dest,
                    });
                } else if compare_points(&edge_dest, &intersection.point).is_eq() {
                    assert!(!origin_equal);

                    // The current edge is inward (opposite of first case).
                    // edge.next MUST also be in the current intersection as well.
                    intersecting_edges.push(PartialEdge {
                        inward_id: edge_id,
                        inward_prev: edge.prev,
                        inward_face: edge.incident_face,
                        outward_id: edge.twin,
                        outward_next: self.half_edges[&edge.twin].next,
                        outward_face: self.half_edges[&edge.twin].incident_face,
                        point: edge.origin.clone(),
                    });
                } else {
                    let id1 = self.next_edge_id;
                    let id2 = self.next_edge_id + EdgeId(1);
                    self.next_edge_id = self.next_edge_id + EdgeId(2);

                    let e1 = PartialEdge {
                        inward_id: edge_id,
                        inward_prev: edge.prev,
                        inward_face: edge.incident_face,
                        outward_id: id1,
                        outward_next: self.half_edges[&edge.twin].next,
                        outward_face: self.half_edges[&edge.twin].incident_face,
                        point: edge.origin.clone(),
                    };

                    let e2 = PartialEdge {
                        inward_id: edge.twin,
                        inward_prev: self.half_edges[&edge.twin].prev,
                        inward_face: self.half_edges[&edge.twin].incident_face,
                        outward_id: id2,
                        outward_next: edge.next,
                        outward_face: edge.incident_face,
                        point: edge_dest.clone(),
                    };

                    // Update the segment to correct to the portion of the original segment which
                    // still remains to be matched below (/ to the right of) the sweep line.
                    segment_edge_ids[segment_idx] =
                        if compare_points(&edge.origin, &edge_dest).is_gt() {
                            edge_id
                        } else {
                            edge.twin
                        };

                    self.half_edges.get_mut(&e1.outward_next).unwrap().prev = id1;
                    self.half_edges.get_mut(&e2.outward_next).unwrap().prev = id2;

                    intersecting_edges.push(e1);
                    intersecting_edges.push(e2);
                }
            }

            // Sort edges by ascending clockwise angle
            intersecting_edges.sort_by(|a, b| {
                let a_dir = &a.point - &intersection.point;
                let b_dir = &b.point - &intersection.point;

                let a_angle = 2. * PI - a_dir.y().atan2(a_dir.x());
                let b_angle = 2. * PI - b_dir.y().atan2(b_dir.x());
                a_angle.partial_cmp(&b_angle).unwrap()
            });

            for (i, edge) in intersecting_edges.iter().enumerate() {
                let last_edge = &intersecting_edges[if i > 0 {
                    i - 1
                } else {
                    intersecting_edges.len() - 1
                }];
                let next_edge = &intersecting_edges[(i + 1) % intersecting_edges.len()];

                // Connect this inward edge to the next outward edge in clockwise order.
                self.half_edges.remove(&edge.inward_id);
                self.half_edges.insert(
                    edge.inward_id,
                    HalfEdge {
                        origin: edge.point.clone(),
                        twin: edge.outward_id,
                        incident_face: edge.inward_face,
                        next: next_edge.outward_id,
                        prev: edge.inward_prev,
                    },
                );

                self.half_edges.remove(&edge.outward_id);
                self.half_edges.insert(
                    edge.outward_id,
                    HalfEdge {
                        origin: intersection.point.clone(),
                        twin: edge.inward_id,
                        incident_face: edge.outward_face,
                        next: edge.outward_next,
                        prev: last_edge.inward_id,
                    },
                );

                if let Some(left_neighbor) = intersection.left_neighbor.clone() {
                    edge_left_neighbors.insert(edge.outward_id, segment_edge_ids[left_neighbor]);
                }
            }
        }

        #[derive(Debug)]
        struct Boundary {
            edges: Vec<EdgeId>,
            is_inner: bool,
            leftmost_vertex: EdgeId,

            // vertices: Vec<Vector2f>,
            parent: Option<usize>,

            // Indices of other boundaries which are children of this boundary.
            children: Vec<usize>,
        }

        let inner_boundary_components =
            |all_boundaries: &[Boundary], boundary: &Boundary| -> Vec<EdgeId> {
                let mut out = vec![];

                // TODO: Iterate over a vec of child index slices to avoid copies.
                let mut pending = boundary.children.clone();
                while let Some(id) = pending.pop() {
                    let b = &all_boundaries[id];
                    out.push(b.leftmost_vertex);
                    pending.extend_from_slice(&b.children);
                }

                out
            };

        let mut boundaries = vec![];
        let mut edge_to_boundary_index = HashMap::new();

        // Find all boundary cycles by traversing all the edges.
        for (edge_id, edge) in self.half_edges.iter() {
            if edge_to_boundary_index.contains_key(edge_id) {
                continue;
            }

            let mut edges = vec![];
            let mut vertices = vec![];

            let mut leftmost_vertex = *edge_id;

            {
                let mut current_id = *edge_id;
                while !edge_to_boundary_index.contains_key(&current_id) {
                    edges.push(current_id);
                    edge_to_boundary_index.insert(current_id, boundaries.len());

                    let edge = &self.half_edges[&current_id];
                    current_id = edge.next;

                    vertices.push(edge.origin.clone());

                    let current_leftmost = &self.half_edges[&leftmost_vertex];

                    if compare_points_x_then_y(&edge.origin, &current_leftmost.origin).is_lt() {
                        leftmost_vertex = current_id;
                    }
                }
            }

            let is_inner = {
                let edge = &self.half_edges[&leftmost_vertex];
                let next_edge = &self.half_edges[&edge.next];
                let prev_edge = &self.half_edges[&edge.prev];

                turns_right(&prev_edge.origin, &edge.origin, &next_edge.origin)
            };

            boundaries.push(Boundary {
                edges,
                // vertices,
                is_inner,
                leftmost_vertex,
                // To be populated in the next loop.
                parent: None,
                children: vec![],
            });
        }

        // println!("{:#?}", boundaries);

        // Link all inner boundaries to the boundary immediately to the link of them.
        for i in 0..boundaries.len() {
            let boundary = &boundaries[i];
            if !boundary.is_inner {
                continue;
            }

            let leftmost_edge = &self.half_edges[&boundary.leftmost_vertex];

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

                let mut left_edge = &self.half_edges[&left_edge_id];
                let mut left_edge_dest = self.destination(left_edge);

                // If the left edge is horizontal, instead pick a non-horizontal one with the
                // same edge point as the right side of the horizontal line.
                // TODO: Use a standard constant
                if (left_edge.origin.y() - left_edge_dest.y()).abs() <= 1e-3 {
                    // TODO: Implement a test case which hits thi logic.

                    println!("SKIP HORIZONTAL EDGE");

                    if left_edge.origin.x() > left_edge_dest.x() {
                        left_edge_id = left_edge.prev;
                    } else {
                        left_edge_id = left_edge.next;
                    }

                    left_edge = &self.half_edges[&left_edge_id];
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

        let mut unbounded_face = Face {
            outer_component: None,
            inner_components: vec![],
        };
        let mut unbounded_face_id = FaceId(0);
        let mut next_face_id = FaceId(1);

        let mut faces = HashMap::new();

        // TODO: Also implement transferring of data from the original faces.
        for boundary in &boundaries {
            if boundary.is_inner {
                if boundary.parent.is_some() {
                    // Handled by its parent.
                    continue;
                }

                // Otherwise, this is inside of the unbounded face.
                unbounded_face
                    .inner_components
                    .push(boundary.leftmost_vertex);

                // TODO: When will this be non-zero? (two squares next to each other?)
                // assert_eq!(boundary.children.len(), 0);
                unbounded_face
                    .inner_components
                    .extend(inner_boundary_components(&boundaries, boundary).into_iter());
            } else {
                // Form a new face.

                let face_id = next_face_id;
                next_face_id = next_face_id + FaceId(1);

                faces.insert(
                    face_id,
                    Face {
                        outer_component: Some(boundary.leftmost_vertex),
                        inner_components: inner_boundary_components(&boundaries, boundary),
                    },
                );
            }
        }

        faces.insert(unbounded_face_id, unbounded_face);

        self.faces = faces;
        self.next_face_id = next_face_id;

        println!("{:#?}", self.faces);

        /*
        Labeling new faces with values of old faces:

        - At a single scanline,
            - If our left neighbor if on a hole boundary of the original face,

        - Simple solution:
            - For each new face,
                - Check one point and see if it is inside


        For each  outer boundary:
        - Look with boundary immediately to the left
        - If it's an inner boundary, recursively look with it's left boundary
        - Else, we found a face containing our face
            - Look for it's twin





        Linking new faces to old faces:
        - For each edge in the face's outer component, if it also exists in the original data structure (has same edge id), copy the origin edge's incident face data.
            - The newly creates edges can also be remapped back to their original faces.

        More complicated question is how to determine the composition of the nested polygons:
        - Iterate outwards

        If I'm an outer boundary.
        - Check if I can reach an outer boundary before hitting an inner boundary?

        */

        // Find

        // for

        // List all boundary cycles
        // - Determine the left most (or bottom most) vertex of each
        // - Determine if it an outer or inner (hole) boundary

        // Link all hole boundaries in a graph to the face immediately to the
        // left of it (or the unbounded face)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct EdgeId(usize);

impl Add for EdgeId {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FaceId(usize);

impl Add for FaceId {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

/*
For each intersection point, it is useful to know which original segment it comes from.
- Other things:
    - Don't want to double count line segments if we already read out its twin.

*/

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

fn vec2f(x: f32, y: f32) -> Vector2f {
    Vector2f::from_slice(&[x, y])
}

// Validates the correctness of the HalfEdgeStruct and extracts all boundary
// cycles starting at any edges.
fn get_all_boundaries(data: &HalfEdgeStruct) -> Vec<Vec<Vector2f>> {
    let mut output = vec![];

    let mut seen_ids = HashSet::new();

    for (edge_id, edge) in &data.half_edges {
        assert_eq!(data.half_edges[&edge.next].prev, *edge_id);
        assert_eq!(data.half_edges[&edge.prev].next, *edge_id); //
        assert_eq!(data.half_edges[&edge.twin].twin, *edge_id);

        if seen_ids.contains(edge_id) {
            continue;
        }

        let mut boundary = vec![];
        let mut current_id = *edge_id;
        while seen_ids.insert(current_id) {
            let current_edge = &data.half_edges[&current_id];

            // Edges along a boundary should all be pointing in the same direction.
            let prev_dest = data.destination(&data.half_edges[&current_edge.prev]);
            assert_eq!(prev_dest, current_edge.origin);

            boundary.push(current_edge.origin.clone());
            current_id = current_edge.next;
        }

        output.push(boundary);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_lines_intersect() {
        let mut data = HalfEdgeStruct::new();

        let e1 = data.new_edge_id();
        let e2 = data.new_edge_id();
        let e3 = data.new_edge_id();
        let e4 = data.new_edge_id();

        data.half_edges.insert(
            e1,
            HalfEdge {
                origin: vec2f(0., 0.),
                twin: e2,
                next: e2,
                prev: e2,
                incident_face: FaceId(0), // TODO
            },
        );
        data.half_edges.insert(
            e2,
            HalfEdge {
                origin: vec2f(10., 10.),
                twin: e1,
                next: e1,
                prev: e1,
                incident_face: FaceId(0), // TODO
            },
        );
        data.half_edges.insert(
            e3,
            HalfEdge {
                origin: vec2f(10., 0.),
                twin: e4,
                next: e4,
                prev: e4,
                incident_face: FaceId(0), // TODO
            },
        );
        data.half_edges.insert(
            e4,
            HalfEdge {
                origin: vec2f(0., 10.),
                twin: e3,
                next: e3,
                prev: e3,
                incident_face: FaceId(0), // TODO
            },
        );

        data.repair();

        println!("{:#?}", data);

        let mut seen_ids = HashSet::new();

        // println!("NUM EDGES: {}", data.half_edges.len());

        let mut current_id = *data.half_edges.iter().next().unwrap().0;
        while seen_ids.insert(current_id) {
            let edge = &data.half_edges[&current_id];

            // println!("{:?}", edge.origin);
            current_id = edge.next;
        }

        /*
        This should produce 8 edges which form a single boundary of the form:
        5.0, 5.0,
        10.0, 10.0,
        5.0, 5.0,
        0.0, 10.0,
        5.0, 5.0,
        0.0, 0.0,
        5.0, 5.0,
        10.0, 0.0,
        */
    }

    #[test]
    fn noop_for_two_lines() {
        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));

        data.repair();

        for i in 0..4 {
            println!("{} => {:#?}", i, &data.half_edges[&EdgeId(i)]);
        }

        // println!("{:#?}", data);
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

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));
        let a2 = data.add_next_edge(a1, vec2f(0., 10.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(5., 5.), vec2f(15., 5.));
        let b1 = data.add_next_edge(b0, vec2f(15., 15.));
        let b2 = data.add_next_edge(b1, vec2f(5., 15.));
        data.add_close_edge(b2, b0);

        data.repair();

        println!("NUM EDGES: {}", data.half_edges.len());

        assert_eq!(data.half_edges.len(), 24);

        let boundaries = get_all_boundaries(&data);
        println!("{:#?}", boundaries);
    }

    #[test]
    fn square_inside_square() {
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

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(20., 0.));
        let a1 = data.add_next_edge(a0, vec2f(20., 20.));
        let a2 = data.add_next_edge(a1, vec2f(0., 20.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(5., 5.), vec2f(15., 5.));
        let b1 = data.add_next_edge(b0, vec2f(15., 15.));
        let b2 = data.add_next_edge(b1, vec2f(5., 15.));
        data.add_close_edge(b2, b0);

        data.repair();
    }

    #[test]
    fn adjacent_squares() {
        let mut data = HalfEdgeStruct::new();

        let a0 = data.add_first_edge(vec2f(0., 0.), vec2f(10., 0.));
        let a1 = data.add_next_edge(a0, vec2f(10., 10.));
        let a2 = data.add_next_edge(a1, vec2f(0., 10.));
        data.add_close_edge(a2, a0);

        let b0 = data.add_first_edge(vec2f(15., 5.), vec2f(25., 5.));
        let b1 = data.add_next_edge(b0, vec2f(25., 15.));
        let b2 = data.add_next_edge(b1, vec2f(15., 15.));
        data.add_close_edge(b2, b0);

        data.repair();
    }
}
