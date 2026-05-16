
/*
TODO: Test this.
*/

use math::matrix::{Vector3f, Vector2f};
use math::matrix::vec3f;

use crate::solver::*;
use crate::camera::*;
use crate::pnp::project_point;

pub struct CameraInstrinsicsSolver {
    solver: NonLinearSolver<'static>,
    intrinsics_block: usize,
}

impl CameraInstrinsicsSolver {

    pub fn new(initial_model: CameraIntrinsicsModel) -> Self {
        let mut solver = NonLinearSolver::new();
        solver.set_use_gradient_descent();

        // TODO: Need to clamp the values range.
        let intrinsics_block = solver.add_parameter_block(
            &initial_model.serialize(),
            CameraIntrinsicsParameterBlock::new(initial_model.clone())
        );

        Self {
            solver,
            intrinsics_block
        }
    }

    pub fn add_object(
        &mut self,
        initial_rotation: Vector3f,
        initial_translation: Vector3f, 
        points_3d: &[Vector3f],
        points_2d: &[Vector2f]
    ) {
        let r_param = self.solver.add_parameter_block(&[
            initial_rotation[0], initial_rotation[1], initial_rotation[2],
        ], AxisAngleParameterBlock::default());

        let t_param = self.solver.add_parameter_block(&[
            initial_translation[0], initial_translation[1], initial_translation[2]
        ], LinearParameterBlock::default());

        for i in 0..points_3d.len() {
            self.solver.add_residual_block(&[ r_param, t_param, self.intrinsics_block ], ReprojectionResidual {
                point_3d: points_3d[i].clone(),
                point_2d: points_2d[i].clone()
            });
        }
    }

    pub fn solve(&self) -> CameraIntrinsicsModel {

        let solution = self.solver.solve();

        // println!("{:?}", solution);

        CameraIntrinsicsModel::parse(&solution.params[0..6])
    }

}


struct ReprojectionResidual {
    point_3d: Vector3f,
    point_2d: Vector2f,
}

impl ReprojectionResidual {
    fn calc_error(
        &self,
        params: &[f32],
        axis_angle: &Vector3f,
    ) -> f32 {
        let small_axis_angle = vec3f(params[0], params[1], params[2]);
        let translation = vec3f(params[3], params[4], params[5]);
        let intrinsics = CameraIntrinsicsModel::parse(&params[6..(6 + 6)]);

        let pt3 = &self.point_3d;
        let pt2 = &self.point_2d;

        let projected = project_point(pt3, &intrinsics, axis_angle, &small_axis_angle, &translation);

        // TODO: technically here we are outputting two residuals and not 1
        // TODO: Check if norm or norm_squared is better.
        (projected - pt2).norm_squared()
    }
}

impl ResidualBlockFunction for ReprojectionResidual {
    fn len(&self) -> usize {
        1
    }

    fn calculate(&self, params: &[f32], out: &mut [f32], gradient: &mut [f32]) {
        let axis_angle = vec3f(params[0], params[1], params[2]);
        
        let mut expanded_params = params.to_vec();
        expanded_params[0..3].fill(0.0);

        out[0] = self.calc_error(&expanded_params, &axis_angle);
 
        let step = 0.0001;
        for i in 0..params.len() {
            let v = expanded_params[i];
            expanded_params[i]  = v + step;

            let error1 = self.calc_error(&expanded_params, &axis_angle);

            expanded_params[i]  = v - step;
            let error2 = self.calc_error(&expanded_params, &axis_angle);

            expanded_params[i] = v;

            // negative since we want the gradient of project_point not the error function.
            gradient[i] = -(error1 - error2) / (2.0 * step);
        }
    }
}