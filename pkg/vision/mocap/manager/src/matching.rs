use std::collections::HashMap;
use std::time::Duration;

use common::errors::*;
use common::hash::*;
use common::fixed::vec::FixedVec;
use vision::*;
use math::matrix::{Vector2d, Vector3d, Matrix3d, vec3d, vec2d};
use math::matrix::axis_angle::*;
use math_proto_util::*;
use mocap_proto::mocap::*;

use crate::util::*;
use crate::kalman::*;
use crate::alpha_beta::*;

/*

TODOs:
- If we previously triangulated a points with 3 cameras, allow it to be triangulated with fewer cameras if we still see it nearby
- After performing all triangulations, throw out any 2d points that might be confused between 2 3d points and then re-apply triangulation gradient descent of effected points.

TODO:
- The order in which we try cameras matters
- If two cameras are close to each other, then when triangulating a point between them, a small error in pixel positions will amplify into a large error in distance to the camera. 
    - If validating with a third camera, then that camera also needs to be relatively far away to get a good confirmation
    - But rotation also needs to be considered since two cameras opposite each other (at 180) are also low confidence.
- Ideally based on the angle between cameras (or covariance matrix), we filter matches based on confidence directly.

Some ideas for speeding up:
- Early reject 2 or 3 point tracks if they have low confidence.
- Sort the camera list by distance to each other (greedily try distant cams first)
- Once we have 3 points, re-triangulate to get a better fit.
- Need to sort tracked point proposals based on confidence.
    - (better to have 3 diverse views with high error than three close views with very low error)

TODO: At the end of triangulation, need automatic merging of any points that are <1cm apart from each other.


*/


#[derive(Clone)]
pub struct CameraParameters {
    pub id: u64,

    pub intrinsics: CameraIntrinsicsModel,

    pub extrinsics: CameraExtrinsics,
}

/// Performs matching up of 2d blobs across cameras at a single point in time.
/// The output result is 3d points triangulated from these points.
///
/// The algorithm is as follows:
/// - Loop over all blobs in all cameras
///   - Loop through other cameras
///     - Try to find another blob that matches the first blob based on proximity to the
///       epipolar line.
///     - If we find one,
///       - Triangulate a 3d point using these 2 blobs.
///       - Loop over all remaining cameras and grab blobs that are close enough to the 2d
///         projection of the 3d point we just triangulated (picking the best blob per camera
///         if there are multiple good options)
///       - If we found enough matches across cameras, re-triangulate and output the 3d point
///         - Also mark all blobs for this point as claimed (won't be considered in future
///           matching rounds).  
///
/// TODO: Also filter based on relative radius of points and eventually estimate the 3d radius of the points.
///
/// NOTE: Internally all camera_ids are normalized to constant indexes
pub struct BlobMatcher {
    config: BlobMatchingConfig,

    camera_params: Vec<CameraParameters>,

    camera_id_to_index: HashMap<u64, usize, FastHasherBuilder>,

    essential_mats: TupleVec<Matrix3d>,

    last_point_id: u64,

    /// Points saved from the last time we ran this.
    last_3d_points: Vec<TrackedPointState>,

    /// Mapping from ids to indexes in last_3d_points.
    last_id_to_index: HashMap<u64, usize, FastHasherBuilder>,

    /// Timestamp of the last processed frames.
    last_predicted_time: u64,

    /// For each camera, a list of normalized/undistorted points for the current
    /// point in time being analyzed.
    /// 
    /// (working state for the current frames being processed)
    current_2d_points: Vec<Vec<Point2dEntry>>,

    /// (working state for the current frames being processed)
    current_3d_points: Vec<TrackedPointState>,
}

struct TrackedPointState {
    id: u64,

    position: Vector3d,

    /// List of camera ids that observed this point in the most recent frame.
    ///
    /// This will be empty if we didn't see this point in the most recent frame but
    /// we still predict that it probably exists. 
    ///
    /// TODO: Just use a bitmap.
    camera_ids: Vec<u64>,

    predictor: AlphaBetaEstimator3D,

    last_observed_time: u64,
}

impl TrackedPointState {
    fn to_public_point(&self) -> TrackedPoint {
        TrackedPoint {
            id: self.id,
            // TODO: Maybe output both raw position and smoothed one.
            position: self.predictor.x().clone(),
            camera_ids: self.camera_ids.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TrackedPoint {
    pub id: u64,
    
    pub position: Vector3d,

    pub camera_ids: Vec<u64>,
}


impl TrackedPoint {
    pub fn from_proto(proto: &TrackedPointProto) -> Result<Self> {
        Ok(Self {
            id: proto.id(),
            position: Vector3d::from_proto(proto.position())?,
            camera_ids: proto.camera_ids().to_vec()
        })
    }

    pub fn to_proto(&self) -> TrackedPointProto {
        let mut proto = TrackedPointProto::default(); // proto.new_points();
        proto.set_id(self.id);

        // TODO: Perform real radius estimation and account for it in triangulation.
        proto.set_radius(0.02);

        proto.set_position(self.position.to_proto());

        for id in &self.camera_ids {
            proto.add_camera_ids(*id);
        }

        proto
    }
}

struct TrackedPointProposal {
    observations: Vec<(usize, usize)>,
    position: Vector3d,
    score: f64,
}

struct Point2dEntry {
    raw_point: Vector2d,
    normalized_point: Vector2d,
    claimed: bool,
}

impl BlobMatcher {

    pub fn new(config: &BlobMatchingConfig) -> Self {
        Self {
            config: config.clone(),
            camera_params: Default::default(),
            camera_id_to_index: Default::default(),
            essential_mats: Default::default(),
            last_point_id: 0,
            last_3d_points: vec![],
            last_id_to_index: Default::default(),
            last_predicted_time: 0,
            current_2d_points: vec![],
            current_3d_points: vec![],
        }
    }

    pub fn set_camera_parameters(&mut self, camera_params: &[CameraParameters]) {
        self.camera_params = camera_params.to_vec();

        // Want the matching to be fully deterministic.
        self.camera_params.sort_by_key(|p| p.id);

        self.camera_id_to_index = HashMap::default();
        self.camera_id_to_index.reserve(self.camera_params.len());

        self.current_2d_points = vec![];
        self.current_2d_points.reserve_exact(self.camera_params.len());

        for (i, params) in self.camera_params.iter().enumerate() {
            assert!(self.camera_id_to_index.insert(params.id, i).is_none());
            self.current_2d_points.push(vec![]);
        }

        self.essential_mats = TupleVec::new(Matrix3d::zero(), self.camera_params.len());
        for i in 0..self.camera_params.len() {
            for j in (i + 1)..self.camera_params.len() {
                self.essential_mats.set(i, j, vision::essential_matrix(
                    &self.camera_params[i].extrinsics, &self.camera_params[j].extrinsics));
            }
        }
    } 

    pub fn num_unique_tracks(&self) -> usize {
        self.last_point_id as usize
    }

    pub fn run(&mut self, results: &ReadBlobsResponse) -> Vec<TrackedPoint> {
        self.current_3d_points.clear();

        // Pre-calculate all normalized and undistorted points.
        // This populates 'current_2d_points'
        {
            for v in self.current_2d_points.iter_mut() {
                v.clear();
            }

            for camera in results.cameras() {
                let idx = match  self.camera_id_to_index.get(&camera.camera_id()) {
                    Some(v) => *v,
                    None => continue
                };

                let params = &self.camera_params[idx];
                let out = &mut self.current_2d_points[idx];
                out.reserve_exact(camera.results().blobs().len());

                for blob in camera.results().blobs() {
                    let raw_point = vec2d(blob.x() as f64, blob.y() as f64);
                    let normalized_point = params.intrinsics.unproject_point(&raw_point);

                    out.push(Point2dEntry {
                        normalized_point,
                        raw_point,
                        claimed: false,
                    });
                }
            }
        }

        let mut proposals = vec![];

        self.propagate_old_points(results, &mut proposals);

        let mut epipolar_matches = vec![];

        for cam1_idx in 0..self.camera_params.len() {
            for pt1_idx in 0..self.current_2d_points[cam1_idx].len() {
                let pt1 = &self.current_2d_points[cam1_idx][pt1_idx];
                if pt1.claimed {
                    continue;
                }

                for cam2_idx in (cam1_idx + 1)..self.camera_params.len() {

                    self.find_epipolar_line_match(cam1_idx, pt1_idx, cam2_idx, &mut epipolar_matches);

                    for (_, pt2_idx) in epipolar_matches.iter().cloned() {
                        let mut track = vec![];
                        let mut track_good = false;
                        track.push((cam1_idx, pt1_idx));
                        track.push((cam2_idx, pt2_idx));

                        let (pt, _) = match self.triangulate_track(&track) {
                            Some(v) => v,
                            None => continue
                        };

                        for cam_n_idx in 0..self.camera_params.len() {
                            if cam_n_idx == cam1_idx || cam_n_idx == cam2_idx {
                                continue;
                            }

                            if let Some(idx) = self.find_point_match(&pt, cam_n_idx) {
                                track.push((cam_n_idx, idx));
                            }
                        }

                        if track.len() >= (self.config.min_num_matches() as usize) {
                            // TODO: Maybe re-triangulate if we got more points and calculate error/confidence.
                            let score = (track.len() as f64 * 100.0);
                            proposals.push(TrackedPointProposal {
                                observations: track,
                                position: pt,
                                score,
                            });
                        }
                    }

                    // if track_good {
                    //     break;
                    // }
                }

                // if track_good {
                //     self.finalize_track(&track, &track_guess);
                //     track_good = false;
                // }
            }
        }

        proposals.sort_by(|a, b| b.score.total_cmp(&a.score));

        for proposal in proposals {
            self.finalize_track(&proposal.observations, &proposal.position);
        }

        // This is done before matching with old points since we
        // only match to old points if there is no ambiguity in the match to
        // new points.
        self.merge_overlapping_points();

        self.match_new_and_old_points();

        self.remove_stale_points();

        self.label_new_points();

        /*
        {
            let mut nearest = 10000.0;

            for i in 0..self.current_3d_points.len() {
                for j in 0..self.current_3d_points.len() {
                    if i == j {
                        continue;
                    }

                    let pt1 = &self.current_3d_points[i];
                    let pt2 = &self.current_3d_points[j];
                    let dist = (&pt1.position - &pt2.position).norm_squared();
                    if dist < nearest {
                        nearest = dist;
                    }
                }
            }

            nearest = nearest.sqrt();
            if nearest < 0.02 {
                println!("[num points: {}] [nearest distance: {}]", self.current_3d_points.len(), nearest);
            }
        }
        */


        // TODO: The smallest markers I am currently using are 14mm diameter so points <= that distance to
        // each other physically can't exist and are probably the same blob.


        core::mem::swap(&mut self.last_3d_points, &mut self.current_3d_points);

        self.last_id_to_index.clear();
        for (i, pt) in self.last_3d_points.iter().enumerate() {
            self.last_id_to_index.insert(pt.id, i);
        }

        self.points()
    }

    pub fn points(&self) -> Vec<TrackedPoint> {
        let mut out = vec![];
        out.reserve_exact(self.last_3d_points.len());
        for pt in &self.last_3d_points {
            out.push(pt.to_public_point());
        }

        out
    }

    pub fn add_ghost(&mut self, pt: Vector3d) -> u64 {
        let id = self.last_point_id + 1;
        self.last_point_id = id;

        let i = self.last_3d_points.len();
        self.last_3d_points.push(TrackedPointState {
            id,
            position: pt.clone(),
            camera_ids: vec![],
            predictor: AlphaBetaEstimator3D::new(&pt, self.config.predictor_alpha(), self.config.predictor_beta(), self.config.max_marker_speed()),
            // TODO: Pass in the current frame time more directly or rename this variable?
            last_observed_time: self.last_predicted_time,
        });

        self.last_id_to_index.insert(id, i);

        id
    }

    /// This is used to add a position observation for a ghost point after matching.
    ///
    /// This has no effect if the point already had a direct camera observation
    /// in the latest frame.
    pub fn add_position_observation(&mut self, id: u64, predicted_pt: Vector3d) {
        let point_idx = match self.last_id_to_index.get(&id) {
            Some(v) => v,
            None => return
        };
        let point = &mut self.last_3d_points[*point_idx];
        if !point.camera_ids.is_empty() {
            return;
        }
        
        point.predictor.update(&predicted_pt);
    }

    fn merge_overlapping_points(&mut self) {

        let min_distance = squared(self.config.min_marker_distance());

        let mut i = 0;
        while i < self.current_3d_points.len() {
            
            // Skip all the old points. These will be merged separately.
            if self.current_3d_points[i].id != 0 {
                i += 1;
                continue;
            }

            let mut j = i + 1;
            while j < self.current_3d_points.len() {
                let pt_i = &self.current_3d_points[i];
                let pt_j = &self.current_3d_points[j];
                let dist = (&pt_i.position - &pt_j.position).norm_squared();
                if dist < min_distance {
                    // TODO: Verify the camera sets don't overlap.

                    // TODO: Re-triangulate

                    let pt_j = self.current_3d_points.swap_remove(j);
                    let pt_i = &mut self.current_3d_points[i];

                    for cam_id in pt_j.camera_ids {
                        if !pt_i.camera_ids.contains(&cam_id) {
                            pt_i.camera_ids.push(cam_id);
                        } else {
                            println!("Overlapping camera: {}", cam_id);
                        }
                    }

                    continue;
                }

                j += 1;
            }

            i += 1;
        }
    }

    /// Does forward prediction of each old point's new position and do fast initial matching
    /// to 2d blobs based on that position before
    fn propagate_old_points(&mut self, current_results: &ReadBlobsResponse, proposals: &mut Vec<TrackedPointProposal>) {
        let dt = Duration::from_nanos(current_results.frame_timestamp() - self.last_predicted_time).as_secs_f64();
        self.last_predicted_time = current_results.frame_timestamp();

        let mut tmp_points = vec![];
        core::mem::swap(&mut tmp_points, &mut self.last_3d_points);

        
        for mut old_point in tmp_points.drain(..) {
            let predicted_position = old_point.predictor.predict(dt);

            let mut track = vec![];

            for cam_idx in 0..self.camera_params.len() {
                // TODO: Allow this to use rougly matching as long as there as not two two observations near each other
                // (or maybe use the kalman filter confidence value as the threshold up to a maximum value)

                if let Some(idx) = self.find_point_match(&predicted_position, cam_idx) {
                    track.push((cam_idx, idx));
                }
            }

            // TODO: Now that we do this again, skip claimed points in the main code path.
            // TODO: Eventually support 1 point re-triangulation?
            if track.len() >= (self.config.min_num_rematches() as usize) {
                // TODO: Deduplicate with finalize_track
                {
                    let (pt, error_sum) = match self.triangulate_track_with_guess(&track, &predicted_position) {
                        Some(v) => v,
                        None => continue
                    };

                    if self.config.fast_rematch_old_points() {
                        old_point.predictor.update(&pt);
                        old_point.position = pt;
                        old_point.last_observed_time = self.last_predicted_time;

                        old_point.camera_ids.clear();
                        for (camera_idx, point_idx) in track.iter().cloned() {
                            old_point.camera_ids.push(self.camera_params[camera_idx].id);
                            self.current_2d_points[camera_idx][point_idx].claimed = true;
                        }

                        self.current_3d_points.push(old_point);

                        continue;
                    }

                    let score = 100.0 * (track.len() as f64) + 50.0;

                    // TODO: Keep metrics on how often we end up using this proposal.

                    // NOTE: If we end up using this, it will end up temporarily being
                    // a new point that gets remerged in 'match_new_and_old_points'
                    proposals.push(TrackedPointProposal {
                        observations: track,
                        position: pt,
                        score
                    });
                }
            }

            // Pass through to output point set. Will be handled again in 'match_new_and_old_points'
            old_point.position = predicted_position;
            old_point.camera_ids.clear();
            self.current_3d_points.push(old_point);
        }

        // Make sure that last_3d_points has a big buffer.
        core::mem::swap(&mut tmp_points, &mut self.last_3d_points);

        // Just for safety.
        self.last_3d_points.clear();
    }

    fn match_new_and_old_points(&mut self) {

        let mut nearest_points = FixedVec::<_, 3>::new();

        // Loop over points from the previous frame
        for i in 0..self.current_3d_points.len() {
            if i >= self.current_3d_points.len() {
                break;
            }

            let old_point = &self.current_3d_points[i];

            // All points from the previous frame are at the beginning of the vector
            // and have a valid id.
            if old_point.id == 0 {
                break;
            }


            nearest_points.clear();
            for j in 0..self.current_3d_points.len() {
                if i == j {
                    continue;
                }

                let pt = &self.current_3d_points[j];

                let dist = (&pt.position - &old_point.position).norm_squared();

                nearest_points.push((dist, j));

                nearest_points.sort_by(|a, b| a.0.total_cmp(&b.0));
                if nearest_points.len() == 3 {
                    nearest_points.pop();
                }
            }

            if nearest_points.is_empty() {
                continue;
            }

            let (mut nearest_distance, nearest_idx) = nearest_points[0];
            nearest_distance = nearest_distance.sqrt();

            // Nearest point is another old point so we won't match with it.
            if self.current_3d_points[nearest_idx].id != 0 {
                continue;
            }

            if nearest_distance > self.config.max_point_jump() {
                continue;
            }

            if nearest_points.len() > 1 {
                let second_nearest_dist = nearest_points[1].0.sqrt();

                if second_nearest_dist / nearest_distance < self.config.min_old_point_matching_ratio() {
                    continue;
                }
            }

            // NOTE: New points are ahead of old points in the vector so
            // swap_remove here should be safe without more index management.
            let new_point = self.current_3d_points.swap_remove(nearest_idx);

            let old_point = &mut self.current_3d_points[i];
            let old_matched = !old_point.camera_ids.is_empty();

            // TODO: Maybe re-triangulate if the old point also had some camera_ids.

            // TODO: Merge with old camera_ids if any.
            old_point.camera_ids.extend(new_point.camera_ids);

            // TODO: Refactor the code so that we only ever call predictor.update() on one place
            if !old_matched {
                old_point.predictor.update(&new_point.position);
            }

            old_point.position = new_point.position;
            old_point.last_observed_time = new_point.last_observed_time;


        }


    }

    fn remove_stale_points(&mut self) {
        let mut i = 0;
        while i < self.current_3d_points.len() {

            let pt = &self.current_3d_points[i];
            
            let age = Duration::from_nanos(self.last_predicted_time - pt.last_observed_time);
            if age > Duration::from_secs_f64(self.config.ghosting_limit_secs()) {
                self.current_3d_points.swap_remove(i);
                continue;
            }

            i += 1;
        }
    }
    
    fn label_new_points(&mut self) {
        for mut pt in &mut self.current_3d_points {
            if pt.id == 0 {
                pt.id = self.last_point_id + 1;
                self.last_point_id = pt.id;
            }
        }
    }

    /// Given a 2d point in camera 1, finds matchins 2d points in camera 2 which lie along 
    /// the epipolar line formed by these camera's relative poses.
    ///
    /// Multiple points may be dumped to 'matched' in best to worst order.
    fn find_epipolar_line_match(
        &self,
        cam1_idx: usize,
        pt1_idx: usize,
        cam2_idx: usize,
        matches: &mut Vec<(f64, usize)>
    ) {
        matches.clear();

        let e = self.essential_mats.get(cam1_idx, cam2_idx);
        let pt1 = &self.current_2d_points[cam1_idx][pt1_idx];

        let mut line = e * to_3d(&pt1.normalized_point);

        // Normalize the lot so that dot products represent geometric distance to the line.
        // TODO: Return None if this is near zero.
        line /= (squared(line[0]) + squared(line[1])).sqrt();

        // The errors are in normalized units, so we need to convert assuming that
        // the focal lengths along x and y are roughly the same.
        let max_error = self.config.max_reprojection_error() / self.camera_params[cam2_idx].intrinsics.focal_length[0];

        let cam2_pts = &self.current_2d_points[cam2_idx];

        for pt2_idx in 0..cam2_pts.len() {
            let pt2 = &cam2_pts[pt2_idx];
            if pt2.claimed {
                continue;
            }

            let error = to_3d(&pt2.normalized_point).dot(&line).abs();
            if error < max_error {
                matches.push((error, pt2_idx));

            }
        }

        matches.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap());
    }

    /// Given a 3d point, finds a 2d point in the camera at cam_idx which is a projection
    /// of the point. 
    ///
    /// This will return the top-1 point nearest the query point.
    fn find_point_match(&self, pt: &Vector3d, cam_idx: usize) -> Option<usize> {

        let params = &self.camera_params[cam_idx];

        // Project into current camera view.
        let pt = rotate_by_axis_angle(pt, &params.extrinsics.rotation) + &params.extrinsics.translation;
        let pt2 = params.intrinsics.project_point(&pt);

        let mut best_idx = None;
        let mut best_error: f64 = 0.0;

        let cam_pts = &self.current_2d_points[cam_idx];
        for cam_pt_idx in 0..cam_pts.len() {
            let cam_pt = &cam_pts[cam_pt_idx];
            if cam_pt.claimed {
                continue;
            }

            let error = (&cam_pt.raw_point - &pt2).norm_squared();
            if best_idx.is_none() || error < best_error {
                best_idx = Some(cam_pt_idx);
                best_error = error;
            }
        }

        let max_error = squared(self.config.max_reprojection_error());
        if best_error > max_error {
            best_idx = None;
        }

        best_idx
    }

    // TODO: Will probably need to check that the point isn't behind any cameras.
    fn triangulate_track(&self, track: &[(usize, usize)]) -> Option<(Vector3d, f64)> {
        let rough_pt = {
            let mut solver = DLTSolver::new(track.len());
            for (camera_idx, point_idx) in track.iter().cloned() {
                solver.add_normalized_view(
                    &self.camera_params[camera_idx].extrinsics,
                    &self.current_2d_points[camera_idx][point_idx].normalized_point
                );
            }

            match solver.solve() {
                Some(v) => v,
                None => return None
            }
        };

        if rough_pt.is_nan() {
            return None;
        }

        self.triangulate_track_with_guess(track, &rough_pt)
    }

    fn triangulate_track_with_guess(&self, track: &[(usize, usize)], rough_pt: &Vector3d) -> Option<(Vector3d, f64)> {
        // TODO: Need outlier protection.
        let mut solver = TriangulationNonLinearSolver::new(&rough_pt);

        for (camera_idx, point_idx) in track.iter().cloned() {
            solver.add_view(
                &self.camera_params[camera_idx].intrinsics,
                &self.camera_params[camera_idx].extrinsics,
                &self.current_2d_points[camera_idx][point_idx].raw_point
            );
        }

        // TODO: Check non-nan here and remove other estimances.
        let (pt, error) = solver.solve();
        if pt.is_nan() {
            return None;
        }

        Some((pt, error))

    }

    /// Add a new point to the output set based on the given set of 2d point observations.
    ///
    /// - Note that 2d points aren't marked as 'claimed' until this function runs.
    /// - This function will do nothing if any of the given 2d points are already claimed.
    fn finalize_track(&mut self, track: &[(usize, usize)], rough_pt: &Vector3d) {
        // Check all points are still free.
        for (camera_idx, point_idx) in track.iter().cloned() {
            if self.current_2d_points[camera_idx][point_idx].claimed {
                return;
            }
        }

        let (pt, _) = match self.triangulate_track_with_guess(track, rough_pt) {
            Some(v) => v,
            None => return
        };
        
        // NOTE: The process by which we construct tracks should ensure that the
        // RMS error is low so there is no need to check it. 
        // // let reprojection_error = self.calculate_reprojection_error(&track, &pt);
        // // println!("- RMS Error: {}", reprojection_error);

        let mut camera_ids = vec![];
        for (camera_idx, point_idx) in track.iter().cloned() {
            camera_ids.push(self.camera_params[camera_idx].id);
            self.current_2d_points[camera_idx][point_idx].claimed = true;
        }
        
        self.current_3d_points.push(TrackedPointState {
            id: 0,
            position: pt.clone(),
            camera_ids,
            predictor: AlphaBetaEstimator3D::new(&pt, self.config.predictor_alpha(), self.config.predictor_beta(), self.config.max_marker_speed()),
            // TODO: Pass in the current frame time more directly or rename this variable?
            last_observed_time: self.last_predicted_time,
        });
    }

    // RMS
    fn calculate_reprojection_error(&self, track: &[(usize, usize)], pt: &Vector3d) -> f64 {
        let mut sum = 0.0;
        
        for (camera_idx, point_idx) in track.iter().cloned() {
            let int = &self.camera_params[camera_idx].intrinsics;
            let ext = &self.camera_params[camera_idx].extrinsics;
            let actual_pt = &self.current_2d_points[camera_idx][point_idx].raw_point;

            let expected_pt = int.project_point(&(rotate_by_axis_angle(pt, &ext.rotation) + &ext.translation));
            sum += (actual_pt - expected_pt).norm_squared();
        }

        (sum / (track.len() as f64)).sqrt()
    }
}

fn to_3d(v: &Vector2d) -> Vector3d {
    vec3d(v[0], v[1], 1.)
}

fn squared(v: f64) -> f64 {
    v * v
}



