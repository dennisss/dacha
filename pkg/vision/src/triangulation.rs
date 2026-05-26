
use math::matrix::{VectorXf, MatrixXf, Vector3f, Matrix3f, Vector2f, vec2f, vec3f};
use math::matrix::axis_angle::*;

use crate::camera::*;
use crate::solver::*;
use crate::extrinsics::*;


pub struct TriangulationNonLinearSolver<'a> {
    solver: NonLinearSolver<'a>,
    param: usize,
}

impl<'a> TriangulationNonLinearSolver<'a> {
    pub fn new(initial_point: &Vector3f) -> Self {
        let mut solver = NonLinearSolver::new();

        let param = solver.add_parameter_block(&[
            initial_point[0], initial_point[1], initial_point[2]
        ], LinearParameterBlock::default());

        Self {
            solver,
            param
        }
    }

    pub fn add_view(
        &mut self,
        intrinsics: &'a CameraIntrinsicsModel,
        extrinsics: &'a CameraExtrinsics,
        point: &'a Vector2f,
    ) {
        self.solver.add_residual_block(&[self.param], ReprojectionResidual {
            intrinsics,
            extrinsics,
            point
        });
    }

    pub fn solve(&self) -> (Vector3f, f32) {
        let solution = self.solver.solve();

        (
            vec3f(solution.params[0], solution.params[1], solution.params[2]),
            solution.error_sum,
        )
    }

}

struct ReprojectionResidual<'a> {
    intrinsics: &'a CameraIntrinsicsModel,
    extrinsics: &'a CameraExtrinsics,
    point: &'a Vector2f,
}

impl<'a> ReprojectionResidual<'a> {
    fn calc_error(&self, params: &[f32]) -> Vector2f {
        let pt3 = vec3f(params[0], params[1], params[2]);
        
        let projected = {
            let mut pt = rotate_by_axis_angle(&pt3, &self.extrinsics.rotation);
            pt += &self.extrinsics.translation;

            self.intrinsics.project_point(&pt)
        };

        let pt2 = self.point;

        // TODO: technically here we are outputting two residuals and not 1
        pt2 - projected
    }
}

impl<'a> ResidualBlockFunction for ReprojectionResidual<'a> {
    fn len(&self) -> usize {
        2
    }

    fn calculate(&self, params: &[f32], out: &mut [f32], gradient: &mut [f32]) {
        let mut params = params.to_vec();

        out.copy_from_slice(self.calc_error(&params).as_ref());
 
        let step = 0.0001;
        for i in 0..params.len() {
            let v = params[i];
            params[i]  = v + step;

            let error1 = self.calc_error(&params);

            params[i]  = v - step;
            let error2 = self.calc_error(&params);

            params[i] = v;

            // negative since we want the gradient of project_point not the error function.
            let grad = (error1 - error2) / (-2.0 * step);
            gradient[i] = grad[0];
            gradient[params.len() + i] = grad[1];
        }
    }
}

