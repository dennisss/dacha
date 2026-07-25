/*
Suppose we have a list of 3d object points that all lie on a plane. Each point is 'M = [x,y,0,1]'

The 2d projections of these points are 'm = [u, v, 1]'

A homography is a 3x3 matrix 'H' which maps between these (up to some scaling factor 's')

s m = H M

See the "A Flexible New Technique for Camera Calibration" paper for calculation information.

Also https://cseweb.ucsd.edu/classes/wi07/cse252a/homography_estimation/homography_estimation.pdf for a better explanation of the homography estimation part.
*/

use std::f64::consts::SQRT_2;

use math::matrix::{vec2d, Vector2d, Matrix3d, MatrixXd, Vector3d, VectorXd};
use math::matrix::svd::SVD;
use math::matrix::cwise_binary_ops::CwiseMulAssign;

use crate::camera::*;

/// Directly computes the homography between the given sets of points without
/// any normalization.
///
/// Note that if the input/output points aren't in a similar range, then this may
/// have instability. 
///
/// NOTE: The given points should ideally be normalized to a similar range.
pub fn find_homography_raw(input_points: &[Vector2d], output_points: &[Vector2d]) -> Matrix3d {
    assert_eq!(input_points.len(), output_points.len());

    let mut a = MatrixXd::zero_with_shape(2 * input_points.len(), 3*3);

    for i in 0..input_points.len() {
        let x = 2 * i;
        let y = x + 1;

        let ip = &input_points[i];
        let op = &output_points[i];

        a[(x, 0)] = -ip.x();
        a[(x, 1)] = -ip.y();
        a[(x, 2)] = -1.0;
        a[(x, 6)] = ip.x() * op.x();
        a[(x, 7)] = ip.y() * op.x();
        a[(x, 8)] = op.x();

        a[(y, 3)] = -ip.x();
        a[(y, 4)] = -ip.y();
        a[(y, 5)] = -1.0;
        a[(y, 6)] = ip.x() * op.y();
        a[(y, 7)] = ip.y() * op.y();
        a[(y, 8)] = op.y();
    }

    let mut svd = SVD::eigen_svd(&a);

    // This is 9x1
    let h = svd.v.col(svd.v.cols() - 1).to_owned();

    Matrix3d::from_slice(h.as_ref())
}


/// Helper function to compute the Hartley normalization transformation for a set of points.
/// Returns the transformation matrix T, its inverse T_inv, and the normalized points.
pub(crate) fn compute_normalization(points: &[Vector2d]) -> (Matrix3d, Matrix3d, Vec<Vector2d>) {
    let n = points.len() as f64;
    assert!(n > 0.0, "Cannot normalize an empty set of points.");

    // 1. Calculate the centroid
    let mut centroid = Vector2d::zero();
    for p in points {
        centroid += p;
    }
    centroid /= n;

    // Calculate the average distance from the centroid
    let mut mean_dist = 0.0;
    for p in points {
        mean_dist += (p - &centroid).norm();
    }
    mean_dist /= n;

    // Scaling factor needed to make the mean distance sqrt(2)
    let scale = if mean_dist > 1e-8 { SQRT_2 / mean_dist } else { 1.0 };

    let t = Matrix3d::from_slice(&[
        scale, 0.0, -scale * centroid.x(),
        0.0, scale, -scale * centroid.y(),
        0.0, 0.0, 1.0,
    ]);

    let inv_scale = 1.0 / scale;
    let t_inv = Matrix3d::from_slice(&[
        inv_scale, 0.0, centroid.x(),
        0.0, inv_scale, centroid.y(),
        0.0, 0.0, 1.0,
    ]);

    let mut norm_points = vec![];
    for p in points {
        norm_points.push((p - &centroid) * scale);
    }

    (t, t_inv, norm_points)
}

/// Computes the homography using Hartley normalization for numerical stability.
pub fn find_homography(input_points: &[Vector2d], output_points: &[Vector2d]) -> Matrix3d {
    assert_eq!(input_points.len(), output_points.len());

    // Normalize
    let (t_in, _, norm_inputs) = compute_normalization(input_points);
    let (_, t_out_inv, norm_outputs) = compute_normalization(output_points);

    let h_norm = find_homography_raw(&norm_inputs, &norm_outputs);

    // De-normalize
    let h_final = t_out_inv * h_norm * t_in;

    h_final
}

/// NOTE: The returned homography is in normalized (unprojected) units. 
pub fn find_camera_homography(
    intrinsics: &CameraIntrinsicsModel,
    points_3d: &[Vector3d],
    points_2d: &[Vector2d],
) -> Matrix3d {

    // TODO: Make this configurable.
    let mut subsampling = 1;
    while points_3d.len() / subsampling > 32 {
        subsampling *= 2;
    }


    let mut input_points = vec![];
    for (i, pt) in points_3d.iter().cloned().enumerate() {
        assert_eq!(pt.z(), 0.0);

        if i % subsampling != 0 {
            continue;
        }

        input_points.push(vec2d(pt.x(), pt.y()));
    }

    let mut output_points = vec![];
    for (i, pt) in points_2d.iter().cloned().enumerate() {
        if i % subsampling != 0 {
            continue;
        }

        output_points.push(intrinsics.unproject_point(&pt));
    }

    let mut h = find_homography(&input_points, &output_points);

    let l1 = h.col(0).norm();
    let l2 = h.col(1).norm();
    h.col_mut(0).cwise_mul_assign(1.0 / l1);
    h.col_mut(1).cwise_mul_assign(1.0 / l2);
    h.col_mut(2).cwise_mul_assign(1.0 / ((l1 + l2) / 2.0));

    // Z must be positive (can't be behind the camera).
    if h[(2, 2)] < 0.0 {
        h *= -1.0;
    }

    h
}

/// Given a set of homographies calculated from objects in various positions / orientations,
/// extracts a guess of the camera focal length and center.
///
/// The units of these will match the units that were 
///
/// Returns the focal lengths and camera center.
pub fn intrinsics_from_homographies(homographies: &[Matrix3d]) -> (Vector2d, Vector2d) {
    fn h_(h: &Matrix3d, i: usize, j: usize) -> f64 {
        h[(j, i)]
    }

    fn v_(h: &Matrix3d, i: usize, j: usize) -> MatrixXd {
        MatrixXd::from_slice_with_shape(1, 6, &[
            h_(h, i, 0) * h_(h, j, 0),
            h_(h, i, 0) * h_(h, j, 1) + h_(h, i, 1) * h_(h, j, 0),
            h_(h, i, 1) * h_(h, j, 1),
            h_(h, i, 2) * h_(h, j, 0) + h_(h, i, 0) * h_(h, j, 2),
            h_(h, i, 2) * h_(h, j, 1) + h_(h, i, 1) * h_(h, j, 2),
            h_(h, i, 2) * h_(h, j, 2)
        ])
    }

    let n = homographies.len();
    let mut v = MatrixXd::zero_with_shape(2 * n, 6);

    for i in 0..n {
        let h = &homographies[i];
        v.row_mut(2 * i).copy_from_slice(v_(h, 0, 1).as_ref());
        v.row_mut(2 * i + 1).copy_from_slice((
            v_(h, 0, 0) - v_(h, 1, 1)
        ).as_ref());
    }

    let mut svd = SVD::eigen_svd(&v);

    // b = [B11, B12, B22, B13, B23, B33]
    let b = svd.v.col(svd.v.cols() - 1).to_owned();

    let b11 = b[0];
    let b12 = b[1];
    let b22 = b[2];
    let b13 = b[3];
    let b23 = b[4];
    let b33 = b[5];

    // Extract the optical center (y-coordinate) and lambda scale factor
    let v0 = (b12 * b13 - b11 * b23) / (b11 * b22 - b12 * b12);
    let lambda = b33 - (b13 * b13 + v0 * (b12 * b13 - b11 * b23)) / b11;

    // Extract focal lengths (alpha = fx, beta = fy)
    let alpha = (lambda / b11).sqrt();
    let beta = (lambda * b11 / (b11 * b22 - b12 * b12)).sqrt();

    // Extract skew factor (gamma) and the optical center (x-coordinate)
    let gamma = -b12 * alpha * alpha * beta / lambda;
    let u0 = gamma * v0 / beta - b13 * alpha * alpha / lambda;

    // println!("GAMMA: {}", gamma);

    (
        vec2d(alpha, beta), // (f_x, f_y)
        vec2d(u0, v0),      // (c_x, c_y)
    )
}

