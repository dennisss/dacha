use std::collections::{HashSet, HashMap};

use common::errors::*;
use common::fixed::vec::*;
use common::hash::*;
use math::matrix::{Matrix3d, Vector3d};
use math::matrix::axis_angle::to_axis_angle;
use math_proto_util::VectorProtoExt;
use mocap_proto::mocap::*;

use crate::matching::*;
use crate::rigid_transform::*;


/*
TODOs for improving speed/robustness

- Need to establish clear behaviors for symmetric triangles (triangles with multiple sides of similar length)
    - Identify these when we initially split rigid bodies into triangles
    - Run on query into the triangles_index for each symmetric triangle (don't amplify the query load)
    - Attempt rigid transform with all valid permutations
    - If multiple are good, pick the transform with minimum tilt

- If a rigid body has multiple similar triangles, maybe compress these ahead of time to reduce the number of triangle queries.

- We currently a match of 3 ids from a previous frame when re-matching to old frame data
    - If we only have 1 or 2 ids from a previous frame, the old frame data is completely discarded
    - Ideally we would preserve this data and prioritize re-matches with these ids
    - (maybe if we previously used an id for a rigid body, if that id is still in the point cloud, disallow making any matches that don't re-use that point)
    - The challenge is that its easy for the same id to end up getting labeled on multiple rigid bodies.

- Once the BlobMatcher supports measuring outputting confidence scores for matches, use that to weight the rigid transform matching.

- Need a KD-tree/voxel index for faster point/triangle lookups.

- Need some concept of empty space (normally rigid bodies shouldn't have any additional points in the point cloud in/near their convex hull)

- We should be able to match a rigid body using just a single camera frame if enough of the points are visible in it.
*/

/// Tracks zero or more rigid bodies in a 3d point cloud across
/// consecutive time stamps.
///
/// The general algorithm is:
/// - Try to match all rigid bodies with previous matches to points with the same ids
///   - Also fill in new matches for previously ocluded points
///   - Exclude all these points from future checks.
/// - If all rigid bodies are matched, exit early.
/// - Build an index of all unique triangles in the remaining raw point cloud.
/// - Loop through rigid bodies
///   - Loop over each triangle in the current rigit body.
///     - Find all good triangle matches
///       - Attempt to identify all other points in the rigid body based on that triangles orientation.
///         (may also need to try using it upside down)
///       - If we find all points, run SVD to find the final transform and verify error is ok.
///
/// Note that we discard large triangles early (larger than any in registered rigid bodies), so
/// we should end up with << n^3 triangles to search through assuming we have n points in the
/// point cloud. 
#[derive(Default)]
pub struct RigidBodyTracker {
    config: RigidBodyTrackerConfig,
    bodies: Vec<RigidBodyData>,
    min_edge_length: f64,
    max_edge_length: f64,
}

struct RigidBodyData {
    id: u32,

    /// Points of the rigid body with zero rotation/translation.  
    points: Vec<Vector3d>,

    /// Triangles extracted from 'points'.
    triangles: Vec<Triangle>,

    /// For each point, this is the id of the last known point
    /// corresponding to that point.
    ///
    /// Note: This may contain values even if last_matched = false if
    /// we are still tracking a few points but not enough to fully track.
    ///
    /// TODO: Prioritize newly matching with any ids in this list before
    /// randomly picking triangles. 
    point_ids: Vec<Option<u64>>,

    /// If present, the current/last frame had this rigid body fully matched
    /// with a valid transformation.
    matched: Option<(Matrix3d, Vector3d)>,
}

// Data related to a potential rigid body match that we are currently
// constructing.
struct RigidBodyCandidate {
    /// This is the same length as 'points' in RigidBodyData and indicates
    /// which of the rigid body points we have found point cloud points for.
    point_ids: Vec<Option<u64>>,

    /// Ids currently claimed by this rigid body.
    used_ids: HashSet<u64, FastHasherBuilder>,

    matched: Option<(Matrix3d, Vector3d)>,

    /// Flattened list of 3d point mappings for this rigid body that we have found.
    /// (this excludes points with high uncertainty (ghosts))
    /// These are used for running find_rigid_transform.
    input_points: Vec<Vector3d>,
    output_points: Vec<Vector3d>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RigidBody {
    pub id: u32,
    pub point_ids: Vec<Option<u64>>,
    pub transform: Option<(Matrix3d, Vector3d)>,
}

impl RigidBody {
    pub fn to_proto(&self) -> RigidBodyProto {
        let mut out = RigidBodyProto::default();
        out.set_id(self.id);

        if let Some((r, t)) = &self.transform {
            out.set_found(true);
            out.set_rotation(to_axis_angle(r).to_proto());
            out.set_translation(t.to_proto());

            for id in self.point_ids.iter().cloned() {
                out.add_point_ids(id.unwrap_or(0));
            }
        }

        out
    }

}

struct PointIndex<'a> {
    points: &'a [TrackedPoint],
    used_ids: HashSet<u64, FastHasherBuilder>,
    id_to_index: HashMap<u64, usize, FastHasherBuilder>
}

impl<'a> PointIndex<'a> {
    // fn find(query_pt: &Vector3d, max_distance: f64) -> Option<&TrackedPoint> {

    // }

}

impl RigidBodyTracker {

    /// NOTE: This is somewhat slow as it precomputes some data
    /// about all rigid bodies.
    pub fn set_config(&mut self, config: RigidBodyTrackerConfig) -> Result<()> {
        self.config = config;

        // Initialize so that triangles get filtered out as we
        // build the rigid body data.
        self.min_edge_length = self.config.body_config().min_matching_edge_length();
        self.max_edge_length = self.config.body_config().max_matching_edge_length();
        
        // TODO: Eventually support propagating data for unchanged bodies.
        self.bodies.clear();

        let mut min_len = 10000.0f64;
        let mut max_len = 0.0f64;

        for body in self.config.bodies() {
            let id = body.id();
            let mut points = vec![];
            for p in body.points() {
                points.push(Vector3d::from_proto(p)?);
            }

            let triangles = self.build_triangle_list2(&points);

            for tri in &triangles {
                for side_len in tri.side_lengths.iter().cloned() {
                    min_len = min_len.min(side_len);
                    max_len = max_len.max(side_len);
                }
            }

            let point_ids = vec![None; points.len()];

            self.bodies.push(RigidBodyData {
                id,
                points,
                triangles,
                point_ids,
                matched: None
            })
        }

        self.min_edge_length = self.min_edge_length
            .max(min_len - self.config.body_config().max_error());

        self.max_edge_length = self.max_edge_length.min(
            max_len + self.config.body_config().max_error()
        );

        Ok(())
    }

    pub fn run(&mut self, raw_points: &[TrackedPoint]) {

        // The points index stores all candidate points that we are allowed to use for
        // forming rigid bodies (and tracks when points are used to prevent re-use).
        let mut points_index = PointIndex {
            points: raw_points,
            used_ids: Default::default(),
            id_to_index: Default::default()
        };

        for (i, pt) in points_index.points.iter().enumerate() {
            points_index.id_to_index.insert(pt.id, i);   
        }


        let mut all_matching = true;

        // Fast re-matching of already tracked bodies.
        for body_i in 0..self.bodies.len() {
            self.bodies[body_i].matched = None;

            // TODO: Ideally cache all this memory across bodies and just clear the fields each time.
            let mut candidate = self.new_candidate(&self.bodies[body_i]);

            if self.rematch_old_ids(body_i, &mut points_index, &mut candidate) {
                self.finalize_match(body_i, &mut points_index, &mut candidate);
            } else {
                self.bodies[body_i].matched = None;

                // Clear this to ensure that they can't conflict with other matches. 
                for i in 0..self.bodies[body_i].point_ids.len() {
                    self.bodies[body_i].point_ids[i] = None;
                }

                all_matching = false;
            }
        }

        if all_matching {
            return;
        }

        // NOTE: At this point, in self.bodies, every entry either has a valid 'matched'/'point_ids') or
        // both are completely None. 

        let triangle_index = TriangleIndex { triangles: self.build_triangle_list(points_index.points) };

        // Newly matching based on similar triangles.
        for body_i in 0..self.bodies.len() {
            // Skip if we already get a complete match for this.
            if self.bodies[body_i].matched.is_some() {
                continue;
            }

            for triangle_i in 0..self.bodies[body_i].triangles.len() {

                // Find all matches in the index
                
                // TODO: Sort by similarity.
                let matches = triangle_index.find(
                    &self.bodies[body_i].triangles[triangle_i],
                    self.config.body_config().max_error()
                );

                for index_triangle in matches {

                    // TODO: Filter out if any of the indices is already used (or ghosts)

                    let mut candidate = self.new_candidate(&self.bodies[body_i]);

                    let matched = self.match_with_triangle(
                        &self.bodies[body_i],
                        &self.bodies[body_i].triangles[triangle_i],
                        &index_triangle,
                        &points_index,
                        &mut candidate,
                    );

                    if matched {
                        self.finalize_match(body_i, &mut points_index, &mut candidate);
                        break;
                    }
                }

                if self.bodies[body_i].matched.is_some() {
                    break;
                }
            }
        }
    }

    /// After this runs, all matched rigid bodies will have all points assigned ids
    /// (which means that the matcher may have newly created points). 
    pub fn backpropagate_predicted_points(&mut self, matcher: &mut BlobMatcher) {
        for body in &mut self.bodies {
            let (r, t) = match body.matched.as_ref() {
                Some(v) => v,
                None => continue
            };

            for i in 0..body.points.len() {
                let predicted_pt = r * &body.points[i] + t;
                
                if let Some(id) = body.point_ids[i] {
                    matcher.add_position_observation(id, predicted_pt);
                } else {
                    body.point_ids[i] = Some(matcher.add_ghost(predicted_pt));
                }
            }
        }
    }

    pub fn bodies(&self) -> Vec<RigidBody> {

        let mut out = vec![];

        for body in &self.bodies {
            out.push(RigidBody {
                id: body.id,
                point_ids: body.point_ids.clone(),
                transform: body.matched.clone()
            });
        }

        out
    }

    fn new_candidate(&self, body: &RigidBodyData) -> RigidBodyCandidate {
        RigidBodyCandidate {
            point_ids: vec![None; body.points.len()],
            used_ids: HashSet::default(),
            input_points: vec![],
            output_points: vec![],
            matched: None,
        }
    }

    /// Attempt to find matches for a rigid body based on the previous ids
    /// used in the last frame. 
    fn rematch_old_ids(
        &self,
        body_i: usize,
        points_index: &PointIndex<'_>,
        candidate: &mut RigidBodyCandidate
    ) -> bool {
        for (i, id) in self.bodies[body_i].point_ids.iter().cloned().enumerate() {
            let id = match id {
                Some(v) => v,
                None => continue
            };

            let idx = match points_index.id_to_index.get(&id) {
                Some(v) => *v,
                None => continue
            };

            let point = &points_index.points[idx];
            let ghost = point.camera_ids.is_empty();

            candidate.point_ids[i] = Some(id);
            candidate.used_ids.insert(id);

            if !ghost {
                candidate.input_points.push(self.bodies[body_i].points[i].clone());
                candidate.output_points.push(point.position.clone());
            }
        }

        if candidate.input_points.len() < (self.config.body_config().min_rematch_points() as usize) {
            return false;
        }

        candidate.matched = Some(find_rigid_transform(&candidate.input_points, &candidate.output_points, &[]));

        let found_more = self.predicted_remaining_points(
            &self.bodies[body_i], &points_index, candidate
        );

        // Re-calculate transform.
        if found_more {
            candidate.matched = Some(find_rigid_transform(&candidate.input_points, &candidate.output_points, &[]));
        }

        // TODO: Check error is still ok (existing points may  have moved)

        true
    }

    /// Given a match between a triangle in the rigid body and the point index,
    /// this attempts to finish building a full candidate.
    fn match_with_triangle(
        &self,
        body: &RigidBodyData,
        body_triangle: &Triangle,
        index_triangle: &Triangle,
        points_index: &PointIndex<'_>,
        candidate: &mut RigidBodyCandidate
    ) -> bool {
        // Extract first three points from the matching triangle.
        for i in 0..3 {
            let index_pt = &points_index.points[index_triangle.point_indexes[i]];
            if points_index.used_ids.contains(&index_pt.id) {
                return false;
            }

            candidate.point_ids[body_triangle.point_indexes[i]] = Some(index_pt.id);
            candidate.used_ids.insert(index_pt.id);

            // NOTE: We only form triangles from non-ghosts so these should all be good
            // points.
            candidate.input_points.push(
                body.points[body_triangle.point_indexes[i]].clone()
            );
            candidate.output_points.push(index_pt.position.clone());
        }

        candidate.matched = Some(
            find_rigid_transform(&candidate.input_points, &candidate.output_points, &[])
        );

        self.predicted_remaining_points(
            body, &points_index, candidate
        );

        // Check if we have enough points for a match
        if candidate.input_points.len() < (self.config.body_config().min_initial_match_points() as usize) {
            return false;
        }

        // TODO:S skip if we didn't get more points.
        // Re-run SVD and (maybe check error)?
        candidate.matched = Some(
            find_rigid_transform(&candidate.input_points, &candidate.output_points, &[])
        );

        // NOTE: We shouldn't need to check for error since the method by which we 
        // picked the points should guarantee we got a good fit.

        true
    }

    /// Assuming that the candidate now has a complete rigid body match and
    /// transform, store the data to record the completion of a match. 
    fn finalize_match(&mut self, body_i: usize, points_index: &mut PointIndex<'_>, candidate: &mut RigidBodyCandidate) {

        self.bodies[body_i].point_ids = candidate.point_ids.clone();
        self.bodies[body_i].matched = candidate.matched.clone();
        
        for id in candidate.used_ids.iter().cloned() {
            points_index.used_ids.insert(id);
        }
    }

    /// Attempt to populate remaining unlabeled points in the rigid body candidate
    /// by finding the nearest points (based on the current transform) in the whole
    /// set of frame points
    ///
    /// This assumes that the rigid body candidate already has an initial transform estimated. 
    fn predicted_remaining_points(
        &self,
        body: &RigidBodyData,
        points_index: &PointIndex<'_>,
        candidate: &mut RigidBodyCandidate
    ) -> bool {
        let (r, t) = candidate.matched.as_ref().unwrap();

        // Find additional points.
        let mut found_more = false;
        for i in 0..candidate.point_ids.len() {
            if candidate.point_ids[i].is_some() {
                continue;
            }

            let predicted_pt = r * &body.points[i] + t; 

            // TODO: Select nearest if there are multiple good matches?
            for pt in points_index.points {
                // TODO: Use squared norm.
                if (&pt.position - &predicted_pt).norm() < self.config.body_config().max_error() {
                    if points_index.used_ids.contains(&pt.id) || candidate.used_ids.contains(&pt.id) {
                        continue;
                    }

                    candidate.point_ids[i] = Some(pt.id);

                    let ghost = pt.camera_ids.is_empty();
                    if !ghost {
                        candidate.input_points.push(body.points[i].clone());
                        candidate.output_points.push(pt.position.clone());
                    }

                    found_more = true;
                    break;
                }
            }
        }

        found_more
    }

    fn build_triangle_list(&self, points: &[TrackedPoint]) -> Vec<Triangle> {
        let mut out = vec![];

        for i in 0..points.len() {
            if points[i].camera_ids.is_empty() {
                continue;
            }

            for j in (i + 1)..points.len() {
                if points[j].camera_ids.is_empty() {
                    continue;
                }

                let pt_i = &points[i].position;
                let pt_j = &points[j].position;
                
                let side_ij = (pt_i - pt_j).norm();
                if !self.filter_side(side_ij) {
                    continue;
                }

                for k in (j + 1)..points.len() {
                    let pt_k = &points[k].position;
                    let side_jk = (pt_k - pt_j).norm();
                    if !self.filter_side(side_jk) {
                        continue;
                    }

                    let side_ki = (pt_k - pt_i).norm();
                    if !self.filter_side(side_ki) {
                        continue;
                    }

                    let mut tri = Triangle {
                        side_lengths: [side_ij, side_jk, side_ki],
                        point_indexes: [i, j, k]
                    };
                    tri.normalize();

                    out.push(tri);
                }
            }
        }

        out
    }

    // TODO: Dedup this with the first one.
    fn build_triangle_list2(&self, points: &[Vector3d]) -> Vec<Triangle> {
        let mut out = vec![];

        for i in 0..points.len() {
            for j in (i + 1)..points.len() {
                let pt_i = &points[i];
                let pt_j = &points[j];
                
                let side_ij = (pt_i - pt_j).norm();
                if !self.filter_side(side_ij) {
                    continue;
                }

                for k in (j + 1)..points.len() {
                    let pt_k = &points[k];
                    let side_jk = (pt_k - pt_j).norm();
                    if !self.filter_side(side_jk) {
                        continue;
                    }

                    let side_ki = (pt_k - pt_i).norm();
                    if !self.filter_side(side_ki) {
                        continue;
                    }

                    let mut tri = Triangle {
                        side_lengths: [side_ij, side_jk, side_ki],
                        point_indexes: [i, j, k]
                    };
                    tri.normalize();

                    out.push(tri);
                }
            }
        }

        out
    }

    fn filter_side(&self, side_length: f64) -> bool {
        side_length >= self.min_edge_length && side_length <= self.max_edge_length
    }

}

#[derive(Clone, Debug)]
struct Triangle {
    side_lengths: [f64; 3],
    point_indexes: [usize; 3]
}

impl Triangle {
    /// Normalizes the triangle such that the side lengths are in sorted order.
    fn normalize(&mut self) {
        let mut expanded_sides = FixedVec::<_, 3>::new();
        for i in 0..3 {
            expanded_sides.push((
                self.side_lengths[i],
                self.point_indexes[i],
                self.point_indexes[(i + 1) % 3]
            ));
        }

        expanded_sides.sort_by(|a, b| a.0.total_cmp(&b.0));

        for i in 0..3 {
            self.side_lengths[i] = expanded_sides[i].0;

            let cur1 = expanded_sides[i].1;
            let cur2 = expanded_sides[i].2;

            let next_i = (i + 1) % 3;
            let next1 = expanded_sides[next_i].1;
            let next2 = expanded_sides[next_i].2;

            if cur1 == next1 || cur1 == next2 {
                self.point_indexes[i] = cur2;
            } else {
                self.point_indexes[i] = cur1;
            }
        }
    }
}


struct TriangleIndex {
    // TODO: Maybe sort and fast filter on the first length.
    triangles: Vec<Triangle>
}

impl TriangleIndex {

    /// NOTE: query_triangle should be normalized.
    ///
    /// TODO: Only return the indexes of points.
    pub fn find(&self, query: &Triangle, error: f64) -> Vec<Triangle> {
        let mut out = vec![];

        for tri in &self.triangles {
            if (tri.side_lengths[0] - query.side_lengths[0]).abs() > error {
                continue;
            }
            if (tri.side_lengths[1] - query.side_lengths[1]).abs() > error {
                continue;
            }
            if (tri.side_lengths[2] - query.side_lengths[2]).abs() > error {
                continue;
            }

            out.push(tri.clone());
        }

        out
    }

}



#[cfg(test)]
mod tests {
    use super::*;

    use math::matrix::vec3d;

    #[test]
    fn works() {
        let mut config = RigidBodyTrackerConfig::default();
        protobuf::text::parse_text_proto(
            r#"
            body_config {
                min_initial_match_points: 3
                min_rematch_points: 3
                min_matching_edge_length: 0.05 # 50mm
                max_matching_edge_length: 10 # 1 meter
                max_error: 0.005 # 5mm
            }
            bodies: [
                {
                    id: 1
                    points: [
                        { values: [ 0, 0, 0 ] },
                        { values: [ 1, 0, 0 ] },
                        { values: [ 0, 2, 0 ] }
                    ]
                }
            ]
            "#,
            &mut config
        ).unwrap();

        let mut tracker = RigidBodyTracker::default();
        tracker.set_config(config).unwrap();

        let points_3d = vec![
            vec3d(0.,  0., 0.),
            // vec3d(1.0, 0., 0.),
            // vec3d(0., 2.0, 0.),
            vec3d(-1., 0.0, 0.),
            vec3d(0., 2.0, 0.),
        ];

        let t = vec3d(0., 0., 3.0);

        let mut tracked_pts = vec![];
        for (i, position) in points_3d.iter().enumerate() {
            tracked_pts.push(TrackedPoint {
                id: (i + 1) as u64,
                position: position.clone() + &t,
                camera_ids: vec![123]
            });
        }


        tracker.run(&tracked_pts);
        println!("{:#?}", tracker.bodies[0].matched);

        tracker.run(&tracked_pts);
        println!("{:#?}", tracker.bodies[0].matched);
    }

}

