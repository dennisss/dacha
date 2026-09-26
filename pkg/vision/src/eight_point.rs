use math::matrix::{Vector2d, Matrix3d, Vector3d, MatrixXd, vec3d};
use math::matrix::svd::SVD;
use math::matrix::axis_angle::*;
use crypto::random::*;

use crate::homography::compute_normalization;
use crate::dlt::*;
use crate::triangulation::*;
use crate::camera::*;
use crate::extrinsics::*;

/*
TODOs:
- Rename to something like PointCorrespondenceExtrinsicsSolver

- Need to return None in solve() if we see that the 


this is bascially a combination of:

- cv2.findEssentialMatrix
- cv2.recoverPose
- scaling translation based on re-triangulated points.
*/

pub struct EightPointCameraExtrinsicsSolver {
    a_intrinsics: CameraIntrinsicsModel,
    b_intrinsics: CameraIntrinsicsModel,
    objects: Vec<Object>
}

struct Object {
    object_points: Vec<Vector3d>,
    a_points: Vec<Vector2d>,
    b_points: Vec<Vector2d>,
}

impl EightPointCameraExtrinsicsSolver {
    pub fn new(
        a_intrinsics: CameraIntrinsicsModel,
        b_intrinsics: CameraIntrinsicsModel,
    ) -> Self {
        Self {
            a_intrinsics,
            b_intrinsics,
            objects: vec![]
        }
    }

    pub fn add_object(
        &mut self,
        object_points: &[Vector3d],
        a_points: &[Vector2d],
        b_points: &[Vector2d],
    
    ) {
        self.objects.push(Object {
            object_points: object_points.iter().cloned().collect(),
            a_points: a_points.iter().cloned().collect(),
            b_points: b_points.iter().cloned().collect(), 
        });
    }

    // pub fn add_point_correspondence(&mut self, a_point: &Vector2d, b_point: &Vector2d) {
    //     self.a_points.push(a_point.clone());
    //     self.b_points.push(b_point.clone());
    // }

    // TODO: Need better failure checking to return none
    pub fn solve(&self) -> Option<CameraExtrinsics> {
        let mut a_normalized = vec![];
        let mut b_normalized = vec![];

        for obj in &self.objects {
            for i in 0..obj.a_points.len() {
                a_normalized.push(self.a_intrinsics.unproject_point(&obj.a_points[i]));
                b_normalized.push(self.b_intrinsics.unproject_point(&obj.b_points[i]));
            }
        }

        if a_normalized.len() < 8 {
            return None;
        }

        let essential_mat = match eight_point_algorithm_ransac(&a_normalized, &b_normalized) {
            Some(v) => v,
            None => return None
        };
        // if essential_mat.is_nan() {
        //     return None;
        // }

        // TODO: Everything below here should run just on inliers.

        // If there is no noise, then for any point:
        // projecting a point into both cameras using only one of these will result
        // in +z coordinates in both camers.
        let candidates = extract_poses_from_essential(&essential_mat);
        let mut candidate_scores = [0usize; 4];

        let mut total_votes = 0;
        for (candidate_i, (r, t)) in candidates.iter().enumerate() {            
            let b_extrinsics = CameraExtrinsics {
                rotation: to_axis_angle(r),
                translation: t.clone()
            };

            for obj in &self.objects {
                for i in 0..obj.a_points.len() {
                    let a_pt3 = match self.triangulate(
                        &b_extrinsics,
                        &self.objects[0].a_points[0],
                        &self.objects[0].b_points[0]
                    ) {
                        Some(v) => v,
                        None => continue
                    };

                    let b_pt3 = b_extrinsics.transform(&a_pt3);
 
                    if a_pt3.z() > 0.0 && b_pt3.z() > 0.0 {
                        candidate_scores[candidate_i] += 1;
                        total_votes += 1;
                    }
                }
            }
        }

        let num_points = a_normalized.len();

        let best_candidate_i = candidate_scores.iter()
            .enumerate()
            .max_by_key(|&(_, item)| item)
            .map(|(index, _)| index).unwrap();

        let best_score = candidate_scores[best_candidate_i];
        if (best_score as f32) < 0.8 * (num_points as f32) || (best_score as f32) < 0.8 * (total_votes as f32) {
            return None;
        }

        let mut extrinsics = CameraExtrinsics {
            rotation: to_axis_angle(&candidates[best_candidate_i].0),
            translation: candidates[best_candidate_i].1.clone()
        };


        // TODO: Do not count any outliers.
        let mut scale = 0.0;
        for obj in &self.objects {
            // TODO: Use the longest distance in each object?

            let pt0 = self.triangulate(&extrinsics, &obj.a_points[0], &obj.b_points[0]).unwrap();
            let pt1 = self.triangulate(&extrinsics, &obj.a_points[1], &obj.b_points[1]).unwrap();

            let dist = (pt1 - pt0).norm();
            let expected_dist = (&obj.object_points[1] - &obj.object_points[0]).norm();

            scale += expected_dist / dist;
        }

        scale /= (self.objects.len() as f64);

        extrinsics.translation *= scale;

        Some(extrinsics)
    }

    fn triangulate(&self, b_extrinsics: &CameraExtrinsics, pt_a: &Vector2d, pt_b: &Vector2d) -> Option<Vector3d> {
        let a_extrinsics = CameraExtrinsics::default();

        let rough_pt = {
            let mut solver = DLTSolver::new(2);
            solver.add_normalized_view(
                &a_extrinsics,
                &self.a_intrinsics.unproject_point(pt_a)
            );
            solver.add_normalized_view(
                &b_extrinsics,
                &self.b_intrinsics.unproject_point(pt_b)
            );

            match solver.solve() {
                Some(v) => v,
                None => return None
            }
        };

        if rough_pt.is_nan() {
            return None;
        }

        let mut solver = TriangulationNonLinearSolver::new(&rough_pt);

        solver.add_view(
            &self.a_intrinsics,
            &a_extrinsics,
            pt_a
        );
        solver.add_view(
            &self.b_intrinsics,
            &b_extrinsics,
            pt_b
        );

        let (pt, error) = solver.solve();
        if pt.is_nan() {
            return None;
        }

        Some(pt)
    }

}

const NUM_ITERS: usize = 500;

// 'pixel_threshold / focal_length'
const INLIER_THRESHOLD: f64 = 1.0 / 50.0;

const MIN_INLIER_PERCENT: f64 = 0.8;

pub fn eight_point_algorithm_ransac(input_points: &[Vector2d], output_points: &[Vector2d]) -> Option<Matrix3d> {

    if input_points.len() < 8 {
        return None;
    }

    let mut rng = MersenneTwisterRng::mt19937();
    rng.seed_u32(123);

    let mut best = None;

    for _ in 0..NUM_ITERS {
        let mut indexes = vec![];
        for i in 0..input_points.len() {
            indexes.push(i);
        }

        rng.shuffle(&mut indexes);

        let mut input_sub_points = vec![];
        let mut output_sub_points = vec![];
        for i in indexes[0..8].iter().cloned() {
            input_sub_points.push(input_points[i].clone());
            output_sub_points.push(output_points[i].clone());
        }

        let mat = eight_point_algorithm(&input_sub_points, &output_sub_points);

        if mat.is_nan() {
            continue;
        }

        let mut num_inliers = 0;
        for i in 0..input_points.len() {
            if check_inlier(&input_points[i], &output_points[i], &mat) {
                num_inliers += 1;
            }
        }


        if let Some((best_num, best_mat)) = &mut best {
            if num_inliers <= *best_num {
                continue;
            }
        }

        best = Some((num_inliers, mat));

        // if (num_inliers as f64) >= (input_points.len() as f64) * MIN_INLIER_PERCENT {
        //     break;
        // }

        // TODO: Stop immediately if we have enough inliers.
    }

    let (num_inliers, mat) = match best {
        Some(v) => v,
        None => return None
    };

    if (num_inliers as f64) < (input_points.len() as f64) * MIN_INLIER_PERCENT {
        return None;
    }


    // Recalculate matrix using all inliers.

    let mut input_inlier_points = vec![];
    let mut output_inlier_points = vec![];

    for i in 0..input_points.len() {        
        if check_inlier(&input_points[i], &output_points[i], &mat) {
            input_inlier_points.push(input_points[i].clone());
            output_inlier_points.push(output_points[i].clone());
        }
    }
    
    // TODO: Figure out why this sometimes happens (implies that the main 8 points aren't inliers)
    if input_inlier_points.len() < 8 {
        return None;
    }

    let mat = eight_point_algorithm(&input_inlier_points, &output_inlier_points);

    Some(mat)
}

fn check_inlier(input_point: &Vector2d, output_point: &Vector2d, mat: &Matrix3d) -> bool {
    let p1 = vec23(input_point);
    let p2 = vec23(output_point);

    let mut line = mat * p1; 

    // Normalize the lot so that dot products represent geometric distance to the line.
    // TODO: Return None if this is near zero.
    line /= (squared(line[0]) + squared(line[1])).sqrt();

    let error = p2.dot(&line).abs();

    error < INLIER_THRESHOLD
}


fn vec23(v: &Vector2d) -> Vector3d {
    vec3d(v.x(), v.y(), 1.0)
}

fn squared(v: f64) -> f64 {
    v * v
}


/// Computes a fundamental/essential matrix that maps between the given set of 2d points.
pub fn eight_point_algorithm(input_points: &[Vector2d], output_points: &[Vector2d]) -> Matrix3d {
    let (t_in, _, norm_inputs) = compute_normalization(input_points);
    let (t_out, _, norm_outputs) = compute_normalization(output_points);

    let f_norm = eight_point_algorithm_raw(&norm_inputs, &norm_outputs);

    // De-normalize
    let f_final = t_out.transpose() * f_norm * t_in;

    f_final
}

fn eight_point_algorithm_raw(input_points: &[Vector2d], output_points: &[Vector2d]) -> Matrix3d {
    assert_eq!(input_points.len(), output_points.len());
    assert!(input_points.len() >= 8);

    let mut subsampling = 1;
    while input_points.len() / subsampling > 64 {
        subsampling *= 2;
    }

    let n = input_points.len() / subsampling;

    let mut a = MatrixXd::zero_with_shape(n, 9);

    for i in 0..n {
        let ip = &input_points[i * subsampling];
        let op = &output_points[i * subsampling];

        let u = ip.x();
        let v = ip.y();
        let u_prime = op.x();
        let v_prime = op.y();

        // Populate the N x 9 matrix A for the equation Af = 0
        a[(i, 0)] = u_prime * u;
        a[(i, 1)] = u_prime * v;
        a[(i, 2)] = u_prime;
        a[(i, 3)] = v_prime * u;
        a[(i, 4)] = v_prime * v;
        a[(i, 5)] = v_prime;
        a[(i, 6)] = u;
        a[(i, 7)] = v;
        a[(i, 8)] = 1.0;
    }

    // Solve Af = 0 using SVD 
    let svd_a = SVD::eigen_svd(&a);
    
    // The solution 'f' is the last column of V (associated with the smallest singular value)
    let f_vec = svd_a.v.col(svd_a.v.cols() - 1).to_owned();
    let mut f_mat = Matrix3d::from_slice(f_vec.as_ref());

    // Rebuilding 'f_mat' to be rank 2.

    let svd_f = SVD::eigen_svd(&f_mat);

    let mut s = Matrix3d::zero();
    s[(0, 0)] = 1.0;
    s[(1, 1)] = 1.0; 
    s[(2, 2)] = 0.0;

    // s[(0, 0)] = svd_f.s[0];
    // s[(1, 1)] = svd_f.s[1];
    
    f_mat = svd_f.u * s * svd_f.v.transpose();

    f_mat
}

/// Extracts the four possible (rotation, translation) pairs from an essential matrix.
/// Note: The translation vector is recovered up to an unknown scale and is unnormalized here.
///
/// Only when using one of these to triangulate a valid pair of 2d points will the z coordinate
/// be positive.
///
/// See
/// https://en.wikipedia.org/wiki/Essential_matrix#Extracting_rotation_and_translation
pub fn extract_poses_from_essential(e: &Matrix3d) -> [(Matrix3d, Vector3d); 4] {
    let svd = SVD::eigen_svd(e);

    let mut u = svd.u;
    let mut v = svd.v;

    // U and V must represent valid rotations, meaning their determinants must be +1.
    // If the determinant is negative, we negate the matrix to flip its parity.
    if u.determinant() < 0.0 {
        u *= -1.0;
    }
    if v.determinant() < 0.0 {
        v *= -1.0;
    }

    // W represents a 90-degree rotation around the Z-axis.
    let w = Matrix3d::from_slice(&[
        0.0, -1.0, 0.0,
        1.0,  0.0, 0.0,
        0.0,  0.0, 1.0,
    ]);

    let w_t = w.transpose();

    // Calculate the two possible rotation matrices
    let r1 = u.clone() * w * v.clone().transpose();
    let r2 = u.clone() * w_t * v.transpose();

    // Extract translation t from the last column of U. 
    // This gives the direction of translation. The magnitude is unrecoverable from E alone.
    let t_slice = u.col(2).to_owned();
    let t1 = Vector3d::from_slice(t_slice.as_ref());
    let t2 = t1.clone() * -1.0; // Negate the vector to get the second translation possibility

    [
        (r1.clone(), t1.clone()),
        (r1, t2.clone()),
        (r2.clone(), t1),
        (r2, t2)
    ]
}

