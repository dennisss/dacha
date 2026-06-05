
use math::matrix::{MatrixStatic, VectorXf, MatrixXf, Vector3f, Matrix3f, Vector2f, vec2f, vec3f};
use math::matrix::axis_angle::*;
use typenum::{U2, U3};

type Matrix2x3f = MatrixStatic<f32, U2, U3>;

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
            translation: extrinsics.translation.clone(),
            rotation: from_axis_angle(&extrinsics.rotation),
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
    rotation: Matrix3f,
    translation: Vector3f,
    point: &'a Vector2f,
}

impl<'a> ReprojectionResidual<'a> {
    /*
    This basically does the following:

    - Parameter is a 3d point in world space
    - Transform into camera space.
    - Project to a 2d point using the camera intrinsics.
    - Calculate the gradients of that.
    */
    fn calc_error(&self, params: &[f32]) -> (Vector2f, Matrix2x3f) {
        // Point in world space.
        let pt_w = vec3f(params[0], params[1], params[2]);

        // Point in camera space
        let mut pt_c = &self.rotation * pt_w;
        pt_c += &self.translation;
        let x_c = pt_c[0];
        let y_c = pt_c[1];
        let z_c = pt_c[2];

        // Perspective
        let z_inv = 1.0 / z_c;
        let x = x_c * z_inv;
        let y = y_c * z_inv;

        // Distortion
        let x2 = x * x;
        let y2 = y * y;
        let r2 = x2 + y2;
        let r4 = r2 * r2;
        let k1 = self.intrinsics.k1;
        let k2 = self.intrinsics.k2;
        
        let d = 1.0 + k1 * r2 + k2 * r4;

        // Calculate final projected point for the residual
        let fx = self.intrinsics.focal_length[0];
        let fy = self.intrinsics.focal_length[1];
        
        let mut projected = vec2f(x * d, y * d);
        projected[0] = projected[0] * fx + self.intrinsics.center[0];
        projected[1] = projected[1] * fy + self.intrinsics.center[1];

        let residual = self.point - projected;

        // 4. Analytical Jacobian Calculation
        // D = 2 * d(d)/d(r^2) = 2 * (k1 + 2 * k2 * r2)
        let d_factor = 2.0 * (k1 + 2.0 * k2 * r2);
        
        // Precompute shared terms for the Jacobian
        let d_plus_dfactor_x2 = d + d_factor * x2;
        let d_plus_dfactor_y2 = d + d_factor * y2;
        let dfactor_xy = d_factor * x * y;
        
        let k = d + d_factor * r2;

        let fx_z_inv = fx * z_inv;
        let fy_z_inv = fy * z_inv;

        let j_cam = Matrix2x3f::from_slice(&[
            fx_z_inv * d_plus_dfactor_x2, 
            fx_z_inv * dfactor_xy,        
            fx_z_inv * (-x * k),
            //
            fy_z_inv * dfactor_xy,        
            fy_z_inv * d_plus_dfactor_y2, 
            fy_z_inv * (-y * k)           
        ]);
        
        let j_total = j_cam * &self.rotation;

        (residual, j_total)
    }
}

impl<'a> ResidualBlockFunction for ReprojectionResidual<'a> {
    fn len(&self) -> usize {
        2
    }

    fn calculate(&self, params: &[f32], out: &mut [f32], gradient: &mut [f32]) {
        let (res, grad) = self.calc_error(params);
        out.copy_from_slice(res.as_ref());
        gradient.copy_from_slice(grad.as_ref());
    }
}

