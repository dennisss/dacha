
use math::matrix::{VectorXf, MatrixXf, Vector3f, Matrix3f, Vector2f, vec2f, vec3f};
use math::matrix::axis_angle::*;

use crate::camera::*;
use crate::solver::*;

// TODO: Have bounds on all the parameter values while doing the solver iterations (e.g. limit max distance from the camera)

/// Finds a 3d transformation that minimizes the reprojection error of a set of points
/// with a pre-calibrated camera.
///
/// This optimizes 6 parameters:
/// - 3d rotation axis-angle vector
/// - 3d translation vector.
struct ReprojectionProblem<'a> {
    intrinsics: &'a CameraIntrinsicsModel,
    points_3d: &'a [Vector3f],
    points_2d: &'a [Vector2f],
}

impl<'a> ReprojectionProblem<'a> {

    fn calc_error(&self, point_idx: usize, params: &[f32]) -> f32 {
        let small_axis_angle = vec3f(params[0], params[1], params[2]);
        let translation = vec3f(params[3], params[4], params[5]);
        let axis_angle = vec3f(params[6], params[7], params[8]);

        let pt3 = &self.points_3d[point_idx];
        let pt2 = &self.points_2d[point_idx];

        let projected = project_point(pt3, &self.intrinsics, &axis_angle, &small_axis_angle, &translation);

        // TODO: technically here we are outputting two residuals and not 1
        (projected - pt2).norm()
    }
}

impl<'a> NonLinearProblem for ReprojectionProblem<'a> {
    fn num_points(&self) -> usize {
        self.points_3d.len()
    }

    fn error(&self, point_idx: usize, params: &[f32], gradient: &mut [f32]) -> f32 {

        let expanded_params = vec![
            0., 0., 0.,
            params[3], params[4], params[5],
            params[0], params[1], params[2],
        ];

        let error = self.calc_error(point_idx, &expanded_params);
 
        let step = 0.0001;
        for i in 0..params.len() {
            let mut params2 = expanded_params.clone();
            params2[i]  = expanded_params[i] + step;

            let error1 = self.calc_error(point_idx, &params2);

            params2[i]  = expanded_params[i] - step;
            let error2 = self.calc_error(point_idx, &params2);

            // negative since we want the gradient of project_point not the error function.
            gradient[i] = -(error1 - error2) / (2.0 * step);
        }


        error
    }

    fn update(&self, step: &VectorXf, params: &mut VectorXf) {
        let increment_axis_angle = vec3f(step[0], step[1], step[2]);
        let cur_axis_angle = vec3f(params[0], params[1], params[2]);

        let new_axis_angle = to_axis_angle(&(
            from_axis_angle(&increment_axis_angle) * from_axis_angle(&cur_axis_angle)));
        for i in 0..3 {
            params[i] = new_axis_angle[i];
        }

        for i in 3..6 {
            params[i] += step[i];
        }
    }
}



fn project_point(
    point: &Vector3f,
    intrinsics: &CameraIntrinsicsModel,
    axis_angle: &Vector3f,
    small_axis_angle: &Vector3f,
    translation: &Vector3f
) -> Vector2f {

    let mut point = rotate_by_axis_angle(point, axis_angle);

    point = rotate_by_axis_angle_near_zero(&point, small_axis_angle);

    point += translation;

    let mut point_2d = vec2f(point[0] / point[2], point[1] / point[2]);

    point_2d *= intrinsics.focal_length;
    point_2d += &intrinsics.center;

    point_2d
}


#[derive(Clone, Debug)]
pub struct SolvePnPResult {
    pub translation: Vector3f,
    pub rotation: Vector3f,
    pub total_reprojection_error: f32,
}


/// NOTE: This will always produce a result so the user will need to check the
/// reprojection error to determine if it is reasonable.  
pub fn solve_pnp(
    points_2d: &[Vector2f],
    points_3d: &[Vector3f],
    intrinsics: &CameraIntrinsicsModel,
    initial_rotation: &Vector3f,
    initial_translation: &Vector3f,
) -> SolvePnPResult {
    assert_eq!(points_2d.len(), points_3d.len());

    let problem = ReprojectionProblem {
        points_2d,
        points_3d,
        intrinsics
    };

    let solution = solve_nonlinear(&[
        initial_rotation[0], initial_rotation[1], initial_rotation[2],
        initial_translation[0], initial_translation[1], initial_translation[2]
    ], &problem);

    let rotation = vec3f(solution.params[0], solution.params[1], solution.params[2]);
    let translation = vec3f(solution.params[3], solution.params[4], solution.params[5]);

    SolvePnPResult {
        translation,
        rotation,
        total_reprojection_error: solution.error_sum
    }
}

