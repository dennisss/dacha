use mocap_proto::mocap::*;
use math::matrix::{Vector3d, Vector2d, vec2d, vec3d, Matrix3d};
use math::geometry::line::Line2;
use vision::*;
use math::matrix::axis_angle::to_axis_angle;
use math::matrix::cwise_binary_ops::CwiseMulAssign;

/// Match found between a set of 2d blobs in a frame and a pre-defined
/// 3d-pattern of multiple points that were projected to form the blobs.
#[derive(Clone, Debug)]
pub struct BlobPatternMatch {
    pub points: Vec<BlobPatternMatchPoint>,
    pub error: f64,
    pub translation: Vector3d,
    pub rotation: Vector3d,
}

#[derive(Clone, Debug)]
pub struct BlobPatternMatchPoint {
    pub reference_point: Vector3d,
    pub blob_index: usize,
}


pub struct BlobPatternFinder<'a> {
    config: &'a WandPatternFinderConfig,
    intrinsics: &'a CameraIntrinsicsModel,
    results: &'a BlobResults,
    undistorted_points: Vec<Vector2d>,
}


impl<'a> BlobPatternFinder<'a> {

    /*
    heauristics for filtering points followed by pnp for final verification
    (also radius matching.)
    */

    pub fn find_t_wand(
        config: &'a WandPatternFinderConfig,
        intrinsics: &'a CameraIntrinsicsModel,
        results: &'a BlobResults
    ) -> Option<BlobPatternMatch> {
        let mut undistorted_points = vec![];

        for i in 0..results.blobs().len() {
            let blob_i = &results.blobs()[i];
            let pt = vec2d(blob_i.x() as f64, blob_i.y() as f64);

            let mut norm = intrinsics.unproject_point(&pt);
            norm.cwise_mul_assign(&intrinsics.focal_length);
            norm += &intrinsics.center;
            undistorted_points.push(norm);
        }

        let finder = Self { config, intrinsics, results, undistorted_points };
        finder.find_t_wand_inner()
    }


    fn distance(&self, i: usize, j: usize) -> f64 {
        (&self.undistorted_points[i] - &self.undistorted_points[j]).norm()
        
        /*
        let a = &self.results.blobs()[i];
        let b = &self.results.blobs()[j];

        (squared((a.x() - b.x()) as f64) + squared((a.y() - b.y()) as f64)).sqrt()
        */
    }

    /// Attempts to find a T-wand pattern in 2d and then calls into verify_t_wand_pnp for
    /// verification.
    fn find_t_wand_inner(&self) -> Option<BlobPatternMatch> {

        // TODO: Distance thresholds should probably factor in blob radius.

        for i in 0..self.results.blobs().len() {
            for j in (i + 1)..self.results.blobs().len() {
                let blob_i = &self.results.blobs()[i];
                let blob_j = &self.results.blobs()[j];

                if !self.blob_distance_ok(self.distance(i, j)) {
                    continue;
                }

                let max_radius = blob_i.radius_a().max(blob_j.radius_a()) as f64;
                let min_radius = blob_i.radius_a().min(blob_j.radius_a()) as f64;

                let radius_ratio = max_radius / min_radius;

                if radius_ratio > self.config.max_radius_ratio() {
                    continue;
                }

                let center_i = &self.undistorted_points[i];
                let center_j = &self.undistorted_points[j];

                let line = Line2::from_points(center_i, center_j);

                // TODO: Check if the 'j + 1' usage here is correct.
                for k in (j + 1)..self.results.blobs().len() {

                    let blob_k = &self.results.blobs()[k];
                    let center_k = &self.undistorted_points[k];

                    let new_max_radius = max_radius.max(blob_k.radius_a() as f64);
                    let new_min_radius = min_radius.min(blob_k.radius_a() as f64);
                    let new_radius_ratio = new_max_radius / new_min_radius;

                    if new_radius_ratio > self.config.max_radius_ratio() {
                        continue;
                    }

                    if !self.blob_distance_ok(self.distance(i, k)) ||
                       !self.blob_distance_ok(self.distance(j, k)) {
                        continue;
                    }

                    let line_distance = line.distance_to_point(&center_k).abs();

                    if line_distance > self.config.collinear_pixel_threshold() {
                        continue;
                    }

                    let mut points_list = [
                        (0.0, i),
                        (1.0, j),
                        (line.t_at_point(&center_k), k)
                    ];
                    points_list.sort_by(|a, b| a.partial_cmp(b).unwrap());

                    let mut points_idxs = points_list.into_iter().map(|(_, idx)| idx).collect::<Vec<usize>>();
                    // println!("{:?}", points_idxs);

                    {
                        let side1_dist = self.distance(points_idxs[0], points_idxs[1]);
                        let side2_dist = self.distance(points_idxs[1], points_idxs[2]);

                        let actual_ratio = normalize_ratio(side1_dist / side2_dist);
                        let expected_ratio = normalize_ratio(self.config.left_arm_length() / self.config.right_arm_length());
                        if normalize_ratio(expected_ratio / actual_ratio) > self.config.max_projection_stretch() {
                            continue;
                        }

                        // Normalize the longer arm to be between [0, 1] indexes since the longer one is
                        // 'probably' the left arm
                        if side1_dist < side2_dist {
                            points_idxs.swap(0, 2);
                        }
                    }

                    let arms_distance = self.distance(points_idxs[0], points_idxs[2]);
                    let expected_bottom_distance = arms_distance * (self.config.bottom_length() / (self.config.left_arm_length() + self.config.right_arm_length()));

                    for l in 0..self.results.blobs().len() {
                        if l == i || l == j || l == k {
                            continue;
                        }

                        let blob_l = &self.results.blobs()[l];
                        let center_l = &self.undistorted_points[l]; // vec2d(blob_l.x() as f64, blob_l.y() as f64);


                        let new_max_radius2 = new_max_radius.max(blob_l.radius_a() as f64);
                        let new_min_radius2 = new_min_radius.min(blob_l.radius_a() as f64);
                        let new_radius_ratio2 = new_max_radius2 / new_min_radius2;

                        if new_radius_ratio2 > self.config.max_radius_ratio() {
                            continue;
                        }

                        // Make sure the bottom arm marker isn't close to being collinear with
                        // the other points
                        let bottom_line_distance = line.distance_to_point(&center_l).abs();
                        if bottom_line_distance < self.config.min_bottom_line_pixel_distance() {
                            continue;
                        }

                        // Checking distance from the middle point in the 3-point line to the
                        // bottom arm marker.
                        let bottom_arm_distance = self.distance(points_idxs[1], l);
                        if normalize_ratio(bottom_arm_distance / expected_bottom_distance) > self.config.max_projection_stretch() {
                            continue;
                        }

                        if !self.blob_distance_ok(self.distance(i, l)) ||
                           !self.blob_distance_ok(self.distance(j, l)) ||
                           !self.blob_distance_ok(self.distance(k, l)) {
                            continue;
                        }

                        // TODO: Just push and pop if it doesn't end up working.
                        let mut full_points_list = points_idxs.clone();
                        full_points_list.push(l);

                        if let Some(m) = self.verify_t_wand_pnp(full_points_list) {
                            return Some(m);
                        }
                    }

                }
            }
        }

        None
    }

    fn blob_distance_ok(&self, v: f64) -> bool {
        v >= self.config.min_blob_center_pixel_distance() && v <= self.config.max_blob_center_pixel_distance()
    }

    /// Tests the given blob set against the wand pattern using PNP error.
    /// This will test go standard and upside down.
    fn verify_t_wand_pnp(&self, mut blob_idxs: Vec<usize>) -> Option<BlobPatternMatch> {

        let mut matches = vec![];

        if let Some(m) = self.verify_t_wand_pnp_once(&blob_idxs) {
            matches.push(m);
        }

        blob_idxs.swap(0, 2);
        if let Some(m) = self.verify_t_wand_pnp_once(&blob_idxs) {
            matches.push(m);
        }

        matches.sort_by(|a, b| {
            a.error.partial_cmp(&b.error).unwrap()
        });

        if matches.len() == 0 {
            return None;
        }

        Some(matches[0].clone())
    }

    fn verify_t_wand_pnp_once(&self, blob_idxs: &[usize]) -> Option<BlobPatternMatch> {
        // TODO: Try with the arm sides flipped if the reprojection error is too high.

        // TODO: We can probably use the dot radius to determine if Z offsets are correct.


        let mut points_2d = vec![];
        for idx in blob_idxs {
            let blob = &self.results.blobs()[*idx];
            points_2d.push(vec2d(blob.x() as f64, blob.y() as f64));
        }

        let mut points_2d_norm = vec![];
        for pt in points_2d.iter() {
            points_2d_norm.push(self.intrinsics.unproject_point(&pt));            
        }

        let c = &self.config;
        let points_3d = vec![
            vec3d(-c.left_arm_length(), 0., 0.),
            vec3d(0.,  0., 0.),
            vec3d(c.right_arm_length(), 0., 0.),
            vec3d(0., -c.bottom_length(), 0.),
        ];

        // Guess the rotation of the wand assuming the camera on top of the wand and
        // aligning the two lines that form the wand in 2d space.
        let initial_rotation_mat = {

            let x_axis = {
                let v = (&points_2d_norm[2] - &points_2d_norm[0]).normalized();
                vec3d(v.x(), v.y(), 0.0)
            };

            let y_axis = {
                let line = Line2::from_points(&points_2d_norm[0], &points_2d_norm[2]);
                let mut v = (line.perp() * line.distance_to_point(&points_2d_norm[3])).normalized();
                v *= -1.0;
                vec3d(v.x(), v.y(), 0.0)
            };

            let mut z_axis = x_axis.cross(&y_axis).normalized();

            let y_axis_aligned = z_axis.cross(&x_axis).normalized();

            let mut r = Matrix3d::zero();
            r.col_mut(0).copy_from(&x_axis);
            r.col_mut(1).copy_from(&y_axis);
            r.col_mut(2).copy_from(&z_axis);

            r
        };

        let initial_rotation = to_axis_angle(&initial_rotation_mat); //  vec3d(0., 0., 0.);
        let initial_translation = vec3d(0., 0., 1.);

        // NOTE: We want to get the best possible pose and not just a good enough to verify that
        // the wand is present since we use the pose data for extrinsics calibration later.
        let mut solver = PnPSolver::new(&self.intrinsics, &points_2d, &points_3d);
        // solver.set_min_error(self.config.max_reprojection_error());

        // NOTE: Homography based estimation doesn't work well if 3 of the 4 points are colinear
        solver.set_initial_extrinsics(&CameraExtrinsics {
            rotation: initial_rotation.clone(),
            translation: initial_translation.clone()
        });

        let solution = solver.solve();

        // println!("Solution: {:#?}", solution);

        if solution.error.is_nan() ||
           solution.error > self.config.max_reprojection_error() {
            return None;
        }

        Some(BlobPatternMatch {
            error: solution.error,
            translation: solution.translation,
            rotation: solution.rotation,
            points: points_3d.into_iter().zip(blob_idxs.iter().cloned()).map(|(reference_point, blob_index)| {
                BlobPatternMatchPoint {
                    reference_point,
                    blob_index
                }
            }).collect()
        })
    }

}


fn normalize_ratio(mut v: f64) -> f64 {
    if v < 1.0 {
        v = 1.0 / v;
    }

    v
}

fn squared(v: f64) -> f64 {
    v * v
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_wand() {

        let mut config = WandPatternFinderConfig::default();
        protobuf::text::parse_text_proto(
            r#"
            max_blob_center_pixel_distance: 500.0
            max_radius_ratio: 2.0
            left_arm_length: 0.25
            right_arm_length: 0.125
            bottom_length: 0.2
            max_projection_stretch: 2.0
            max_reprojection_error: 60.0
            "#,
            &mut config
        ).unwrap();


        let points_2d = vec![
            vec2d(504.4785, 658.56165),
            vec2d(556.65356, 496.08597),
            vec2d(582.66376, 414.84045),
            vec2d(688.6572, 537.59406),
        ];

        let points_3d = vec![
            vec3d(-0.25, 0., 0.),
            vec3d(0.,  0., 0.),
            vec3d(0.125, 0., 0.),
            vec3d(0., -0.2, 0.),
        ];

        let intrinsics = CameraIntrinsicsModel::from_nominal_params(
            1920,
            1200,
            millis(4.35),
            micros(3.),
        );

        let initial_rotation = vec3d(0., 0., 0.);
        let initial_translation = vec3d(0., 0., 2.);

        // TODO:
        /*
        // TODO: Initialize a smart rotation
        // TODO: Try with both positive and negative z (though I can guess the side based on where the bottom arm is)
        let solver = PnPSolver::new();
        let solution = solver.solve(&points_2d, &points_3d, &intrinsics, &initial_rotation, &initial_translation);

        println!("{:?}", solution);

        */
    }
}
