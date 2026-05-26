
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
struct ReprojectionResidual<'a> {
    intrinsics: &'a CameraIntrinsicsModel,
    point_3d: &'a Vector3f,
    point_2d: &'a Vector2f,
}

impl<'a> ReprojectionResidual<'a> {
    fn calc_error(&self, params: &[f32], axis_angle: &Vector3f) -> Vector2f {
        let small_axis_angle = vec3f(params[0], params[1], params[2]);
        let translation = vec3f(params[3], params[4], params[5]);

        let pt3 = self.point_3d;
        let pt2 = self.point_2d;

        let projected = project_point(pt3, &self.intrinsics, axis_angle, &small_axis_angle, &translation);

        // TODO: technically here we are outputting two residuals and not 1
        pt2 - projected
    }
}

impl<'a> ResidualBlockFunction for ReprojectionResidual<'a> {
    fn len(&self) -> usize {
        2
    }

    fn calculate(&self, params: &[f32], out: &mut [f32], gradient: &mut [f32]) {
        let axis_angle = vec3f(params[0], params[1], params[2]);

        let mut expanded_params = params.to_vec();
        expanded_params[0..3].fill(0.0);

        out.copy_from_slice(self.calc_error(&expanded_params, &axis_angle).as_ref());
 
        let step = 0.0001;
        for i in 0..params.len() {
            let v = expanded_params[i];
            expanded_params[i]  = v + step;

            let error1 = self.calc_error(&expanded_params, &axis_angle);

            expanded_params[i]  = v - step;
            let error2 = self.calc_error(&expanded_params, &axis_angle);

            expanded_params[i] = v;

            // negative since we want the gradient of project_point not the error function.
            let grad = (error1 - error2) / (-2.0 * step);
            gradient[i] = grad[0];
            gradient[params.len() + i] = grad[1];
        }
    }
}


pub(crate) fn project_point(
    point: &Vector3f,
    intrinsics: &CameraIntrinsicsModel,
    axis_angle: &Vector3f,
    small_axis_angle: &Vector3f,
    translation: &Vector3f
) -> Vector2f {

    let mut point = rotate_by_axis_angle(point, axis_angle);

    point = rotate_by_axis_angle_near_zero(&point, small_axis_angle);

    point += translation;

    intrinsics.project_point(&point)
}


#[derive(Clone, Debug)]
pub struct PnPSolution {
    pub translation: Vector3f,
    pub rotation: Vector3f,
    pub total_reprojection_error: f32,
}


pub struct PnPSolver {
    _hidden: ()
}

impl PnPSolver {
    pub fn new() -> Self {
        Self {
            _hidden: ()
        }
    }

    // pub fn set_min_error(&mut self, value: f32) {
    //     // self.inner.set_min_error(value);
    //     // todo!()
    // }

    /// NOTE: This will always produce a result so the user will need to check the
    /// reprojection error to determine if it is reasonable.
    pub fn solve(
        &self,
        points_2d: &[Vector2f],
        points_3d: &[Vector3f],
        intrinsics: &CameraIntrinsicsModel,
        initial_rotation: &Vector3f,
        initial_translation: &Vector3f,
    ) -> PnPSolution {
        assert_eq!(points_2d.len(), points_3d.len());

        let mut solver = NonLinearSolver::new();

        let r_param = solver.add_parameter_block(&[
            initial_rotation[0], initial_rotation[1], initial_rotation[2],
        ], AxisAngleParameterBlock::default());

        let t_param = solver.add_parameter_block(&[
            initial_translation[0], initial_translation[1], initial_translation[2]
        ], LinearParameterBlock::default());

        for i in 0..points_2d.len() {
            solver.add_residual_block(&[ r_param, t_param ], ReprojectionResidual {
                intrinsics,
                point_2d: &points_2d[i],
                point_3d: &points_3d[i],
            });
        }


        let solution = solver.solve();

        let rotation = vec3f(solution.params[0], solution.params[1], solution.params[2]);
        let translation = vec3f(solution.params[3], solution.params[4], solution.params[5]);

        PnPSolution {
            translation,
            rotation,
            total_reprojection_error: solution.error_sum
        }
    }
}
