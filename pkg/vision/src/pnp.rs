
use math::matrix::{VectorXf, MatrixXd, Vector3d, Matrix3d, Vector2d, vec2d, vec3d};
use math::matrix::axis_angle::*;

use crate::camera::*;
use crate::solver::*;
use crate::extrinsics::*;

// TODO: Have bounds on all the parameter values while doing the solver iterations (e.g. limit max distance from the camera)

/// Finds a 3d transformation that minimizes the reprojection error of a set of points
/// with a pre-calibrated camera.
///
/// This optimizes 6 parameters:
/// - 3d rotation axis-angle vector
/// - 3d translation vector.
struct ReprojectionResidual<'a> {
    intrinsics: &'a CameraIntrinsicsModel,
    point_3d: &'a Vector3d,
    point_2d: &'a Vector2d,
}

impl<'a> ReprojectionResidual<'a> {
    fn calc_error(&self, params: &[f64], axis_angle: &Vector3d) -> Vector2d {
        let small_axis_angle = vec3d(params[0], params[1], params[2]);
        let translation = vec3d(params[3], params[4], params[5]);

        let pt3 = self.point_3d;
        let pt2 = self.point_2d;

        let projected = project_point(pt3, &self.intrinsics, axis_angle, &small_axis_angle, &translation);

        pt2 - projected
    }
}

impl<'a> ResidualBlockFunction for ReprojectionResidual<'a> {
    fn len(&self) -> usize {
        2
    }

    fn calculate(&self, params: &[f64], out: &mut [f64], gradient: &mut [f64]) {
        let axis_angle = vec3d(params[0], params[1], params[2]);

        let mut expanded_params = params.to_vec();
        expanded_params[0..3].fill(0.0);

        out.copy_from_slice(self.calc_error(&expanded_params, &axis_angle).as_ref());
 
        let step = 0.000001;
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


pub fn project_point(
    point: &Vector3d,
    intrinsics: &CameraIntrinsicsModel,
    axis_angle: &Vector3d,
    small_axis_angle: &Vector3d,
    translation: &Vector3d
) -> Vector2d {

    let mut point = rotate_by_axis_angle(point, axis_angle);

    point = rotate_by_axis_angle_near_zero(&point, small_axis_angle);

    point += translation;

    intrinsics.project_point(&point)
}


#[derive(Clone, Debug)]
pub struct PnPSolution {
    pub translation: Vector3d,

    pub rotation: Vector3d,

    /// RMS reprojection error.
    pub error: f64,
}


pub struct PnPSolver<'a> {
    intrinsics: &'a CameraIntrinsicsModel,
    points_2d: &'a [Vector2d],
    points_3d: &'a [Vector3d],
    initial_extrinsics: Option<CameraExtrinsics> 
}

impl<'a> PnPSolver<'a> {
    pub fn new(
        intrinsics: &'a CameraIntrinsicsModel,
        points_2d: &'a [Vector2d],
        points_3d: &'a [Vector3d],
    ) -> Self {
        assert_eq!(points_2d.len(), points_3d.len());

        Self {
            intrinsics,
            points_2d,
            points_3d,
            initial_extrinsics: None
        }
    }

    pub fn set_initial_extrinsics(&mut self, ext: &CameraExtrinsics) {
        self.initial_extrinsics = Some(ext.clone());
    }

    pub fn set_initial_extrinsics_from_homography(&mut self, h: &Matrix3d) {
        let initial_rotation_mat = {
            let x_axis = h.col(0).to_owned().normalized();
            let y_axis = h.col(1).to_owned().normalized();

            let mut z_axis = x_axis.cross(&y_axis).normalized();

            let mut r = Matrix3d::zero();
            r.col_mut(0).copy_from(&x_axis);
            r.col_mut(1).copy_from(&y_axis);
            r.col_mut(2).copy_from(&z_axis);

            r
        };

        let rotation = to_axis_angle(&initial_rotation_mat);
        let translation = h.col(2).to_owned();

        self.initial_extrinsics = Some(CameraExtrinsics {
            rotation,
            translation
        });
    }

    /// NOTE: This will always produce a result so the user will need to check the
    /// reprojection error to determine if it is reasonable.
    pub fn solve(
        &self,
    ) -> PnPSolution {
        let initial_extrinsics = self.initial_extrinsics.as_ref().unwrap();

        let mut solver = NonLinearSolver::new();

        let r_param = solver.add_parameter_block(
            initial_extrinsics.rotation.as_ref(),
            AxisAngleParameterBlock::default()
        );

        let t_param = solver.add_parameter_block(
            initial_extrinsics.translation.as_ref(),
            LinearParameterBlock::default()
        );

        for i in 0..self.points_2d.len() {
            solver.add_residual_block(&[ r_param, t_param ], ReprojectionResidual {
                intrinsics: &self.intrinsics,
                point_2d: &self.points_2d[i],
                point_3d: &self.points_3d[i],
            });
        }

        let solution = solver.solve();

        let rotation = vec3d(solution.params[0], solution.params[1], solution.params[2]);
        let translation = vec3d(solution.params[3], solution.params[4], solution.params[5]);

        // Calculating RMS with final parameters.
        let error = {
            let mut sum = 0.0;
            let mut n = 0;

            for i in 0..self.points_2d.len() {

                let proj = project_point(
                    &self.points_3d[i],
                    &self.intrinsics,
                    &rotation,
                    &vec3d(0., 0., 0.),
                    &translation
                );

                sum += (proj - &self.points_2d[i]).norm_squared();
                n += 1;
            }

            (sum / (n as f64)).sqrt()
        };

        PnPSolution {
            translation,
            rotation,
            error
        }
    }

}
