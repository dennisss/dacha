use std::collections::HashMap;
use std::time::Duration;

use common::hash::*;
use vision::*;
use math::matrix::{Vector2f, Vector3f, Matrix3f, vec3f, vec2f};
use math::matrix::axis_angle::*;
use mocap_proto::mocap::*;

use crate::util::*;
use crate::kalman::*;

/// Maximum amount of time we will store entries for points which haven't been observed.
/// At 120fps, this is ~6 frames.
const GHOSTING_LIMIT: Duration = Duration::from_millis(50);


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
    last_3d_points: Vec<TrackedPoint>,

    /// Timestamp of the last processed frames.
    last_predicted_time: u64,

    /// For each camera, a list of normalized/undistorted points for the current
    /// point in time being analyzed.
    /// 
    /// (working state for the current frames being processed)
    current_2d_points: Vec<Vec<Point2dEntry>>,

    /// (working state for the current frames being processed)
    current_3d_points: Vec<TrackedPoint>,
}

pub struct TrackedPoint {
    pub id: u64,
    
    pub position: Vector3d,

    /// List of camera ids that observed this point in the most recent frame.
    ///
    /// This will be empty if we didn't see this point in the most recent frame but
    /// we still predict that it probably exists. 
    ///
    /// TODO: Just use a bitmap.
    pub camera_ids: Vec<u64>,

    predictor: KalmanPointFilter3D,

    last_observed_time: u64,
}


impl TrackedPoint {
    pub fn to_proto(&self) -> TrackedPointProto {
        let mut proto = TrackedPointProto::default(); // proto.new_points();
        proto.set_id(self.id);

        // TODO: Perform real radius estimation and account for it in triangulation.
        proto.set_radius(0.02);

        // TODO: Maybe output both raw position and smoothed one.
        // let p = &self.position;
        proto.set_position(self.predictor.x().to_proto());

        for id in &self.camera_ids {
            proto.add_camera_ids(*id);
        }

        proto
    }
}

struct TrackedPointProposal {
    observations: Vec<(usize, usize)>,
    position: Vector3f,
}

struct Point2dEntry {
    raw_point: Vector2f,
    normalized_point: Vector2f,
    claimed: bool,
}

impl BlobMatcher {

    pub fn new(config: &BlobMatchingConfig, camera_params: &[CameraParameters]) -> Self {
        let mut camera_params = camera_params.to_vec();

        // Want the matching to be fully deterministic.
        camera_params.sort_by_key(|p| p.id);

        let mut camera_id_to_index = HashMap::default();
        camera_id_to_index.reserve(camera_params.len());

        let mut current_2d_points = vec![];
        current_2d_points.reserve_exact(camera_params.len());

        for (i, params) in camera_params.iter().enumerate() {
            assert!(camera_id_to_index.insert(params.id, i).is_none());
            current_2d_points.push(vec![]);
        }

        let mut essential_mats = TupleVec::new(Matrix3f::zero(), camera_params.len());
        for i in 0..camera_params.len() {
            for j in (i + 1)..camera_params.len() {
                essential_mats.set(i, j, vision::essential_matrix(
                    &camera_params[i].extrinsics, &camera_params[j].extrinsics));
            }
        }

        Self {
            config: config.clone(),
            camera_params,
            camera_id_to_index,
            essential_mats,
            last_point_id: 0,
            last_3d_points: vec![],
            last_predicted_time: 0,
            current_2d_points,
            current_3d_points: vec![]
        }
    }

    pub fn num_unique_tracks(&self) -> usize {
        self.last_point_id as usize
    }

    pub fn run<'a>(&'a mut self, results: &ReadBlobsResponse) -> &'a [TrackedPoint] {
        self.current_3d_points.clear();

        self.propagate_old_points(results);

        // Pre-calculate all normalized and undistorted points.
        {
            for v in self.current_2d_points.iter_mut() {
                v.clear();
            }

            for camera in results.cameras() {
                let idx = *self.camera_id_to_index.get(&camera.camera_id()).unwrap();

                let params = &self.camera_params[idx];
                let out = &mut self.current_2d_points[idx];
                out.reserve_exact(camera.results().blobs().len());

                for blob in camera.results().blobs() {
                    let raw_point = vec2f(blob.x(), blob.y());
                    let normalized_point = params.intrinsics.unproject_point(&raw_point);

                    out.push(Point2dEntry {
                        normalized_point,
                        raw_point,
                        claimed: false,
                    });
                }
            }
        }

        let mut track = vec![];
        let mut track_good = false;
        let mut track_guess = Vector3f::zero();

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
                        track.clear();
                        track.push((cam1_idx, pt1_idx));
                        track.push((cam2_idx, pt2_idx));

                        let (pt, _) = self.triangulate_track(&track);

                        for cam_n_idx in (cam2_idx + 1)..self.camera_params.len() {
                            if let Some(idx) = self.find_point_match(&pt, cam_n_idx) {
                                track.push((cam_n_idx, idx));
                            }
                        }

                        if track.len() >= (self.config.min_num_matches() as usize) {
                            track_good = true;
                            track_guess = pt;
                            break;
                        }
                    }

                    if track_good {
                        break;
                    }
                }

                if track_good {
                    self.finalize_track(&track, &track_guess);
                    track_good = false;
                }
            }
        }


        self.match_new_and_old_points();

        self.remove_stale_points();

        self.label_new_points();

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

            // println!("[num points: {}] [nearest distance: {}]", self.current_3d_points.len(), nearest.sqrt());
        }


        // TODO: The smallest markers I am currently using are 14mm diameter so points <= that distance to
        // each other physically can't exist and are probably the same blob.


        core::mem::swap(&mut self.last_3d_points, &mut self.current_3d_points);
        &self.last_3d_points[..]
    }

    fn propagate_old_points(&mut self, current_results: &ReadBlobsResponse) {
        let dt = Duration::from_nanos(current_results.frame_timestamp() - self.last_predicted_time).as_secs_f32();
        self.last_predicted_time = current_results.frame_timestamp();

        let mut tmp_points = vec![];
        core::mem::swap(&mut tmp_points, &mut self.last_3d_points);

        for mut old_point in tmp_points.drain(..) {
            let predicted_position = old_point.predictor.predict(dt);

            /*
            let mut track = vec![];

            for cam_idx in 0..self.camera_params.len() {
                // TODO: Allow this to use rougly matching as long as there as not two two observations near each other
                // (or maybe use the kalman filter confidence value as the threshold up to a maximum value)

                if let Some(idx) = self.find_point_match(&predicted_position, cam_idx) {
                    track.push((cam_idx, idx));
                }
            }

            // TODO: Eventually support 1 point re-triangulation?
            if track.len() >= (self.config.min_num_rematches() as usize) {
                // TODO: Deduplicate with finalize_track
                {
                    let (pt, error_sum) = self.triangulate_track_with_guess(&track, &predicted_position);

                    // if error_sum > 1.5 {
                    //     return;
                    // }

                    old_point.predictor.update(&pt);
                    old_point.position = pt;
                    old_point.last_observed_time = self.last_predicted_time;

                    old_point.camera_ids.clear();
                    for (camera_idx, point_idx) in track.iter().cloned() {
                        old_point.camera_ids.push(self.camera_params[camera_idx].id);
                        self.current_2d_points[camera_idx][point_idx].claimed = true;
                    }

                    self.current_3d_points.push(old_point);
                }
                
                continue;
            }
            */

            // Not sufficiently visible.
            // old_point.position = predicted_position;
            old_point.camera_ids.clear();

            /*
            let age = Duration::from_nanos(current_results.frame_timestamp() - old_point.last_observed_time);
            if age <= GHOSTING_LIMIT {
                self.current_3d_points.push(old_point);
            }
            */

            self.current_3d_points.push(old_point);
        }

        // Make sure that last_3d_points has a big buffer.
        core::mem::swap(&mut tmp_points, &mut self.last_3d_points);

        // Just for safety.
        self.last_3d_points.clear();
    }

    fn match_new_and_old_points(&mut self) {

        // TODO: FixedVec with 3 entries.
        let mut nearest_points = vec![];

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

                nearest_points.sort_by(|a, b| a.partial_cmp(b).unwrap());
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

            if nearest_distance > 0.05 {
                continue;
            }

            /*
            if nearest_points.len() > 1 {
                let second_nearest_dist = nearest_points[1].0.sqrt();

                if second_nearest_dist / nearest_distance < 2.0 {
                    continue;
                }
            }
            */

            // NOTE: New points are ahead of old points in the vector so
            // swap_remove here shoudl be safe without more index management.
            let new_point = self.current_3d_points.swap_remove(nearest_idx);

            let old_point = &mut self.current_3d_points[i];
            old_point.camera_ids = new_point.camera_ids;
            old_point.predictor.update(&new_point.position);
            old_point.position = new_point.position;
            old_point.last_observed_time = new_point.last_observed_time;


        }


    }

    fn remove_stale_points(&mut self) {
        let mut i = 0;
        while i < self.current_3d_points.len() {

            let pt = &self.current_3d_points[i];
            
            let age = Duration::from_nanos(self.last_predicted_time - pt.last_observed_time);
            if age > GHOSTING_LIMIT {
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
    fn triangulate_track(&self, track: &[(usize, usize)]) -> (Vector3d, f64) {
        let rough_pt = {
            let mut solver = DLTSolver::new(track.len());
            for (camera_idx, point_idx) in track.iter().cloned() {
                solver.add_normalized_view(
                    &self.camera_params[camera_idx].extrinsics,
                    &self.current_2d_points[camera_idx][point_idx].normalized_point
                );
            }

            solver.solve()
        };

        self.triangulate_track_with_guess(track, &rough_pt)
    }

    fn triangulate_track_with_guess(&self, track: &[(usize, usize)], rough_pt: &Vector3d) -> (Vector3d, f64) {
        // TODO: Need outlier protection.
        let mut solver = TriangulationNonLinearSolver::new(&rough_pt);

        for (camera_idx, point_idx) in track.iter().cloned() {
            solver.add_view(
                &self.camera_params[camera_idx].intrinsics,
                &self.camera_params[camera_idx].extrinsics,
                &self.current_2d_points[camera_idx][point_idx].raw_point
            );
        }

        solver.solve()

    }

    fn finalize_track(&mut self, track: &[(usize, usize)], rough_pt: &Vector3f) {
        let (pt, error_sum) = self.triangulate_track_with_guess(track, rough_pt);
        
        // eprintln!("- Error: {}", error_sum / (track.len() as f32));

        // if error_sum > 1.5 {
        //     return;
        // }

        // let id = self.last_point_id + 1;
        // self.last_point_id = id;

        let mut camera_ids = vec![];
        for (camera_idx, point_idx) in track.iter().cloned() {
            camera_ids.push(self.camera_params[camera_idx].id);
            self.current_2d_points[camera_idx][point_idx].claimed = true;
        }

        self.current_3d_points.push(TrackedPoint {
            id: 0,
            position: pt.clone(),
            camera_ids,
            predictor: KalmanPointFilter3D::new(&pt),
            // TODO: Pass in the current frame time more directly or rename this variable?
            last_observed_time: self.last_predicted_time,
        });
    }
}

fn to_3d(v: &Vector2d) -> Vector3d {
    vec3d(v[0], v[1], 1.)
}

fn squared(v: f64) -> f64 {
    v * v
}



