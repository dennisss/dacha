use math::matrix::{VectorXf, MatrixXf, Vector3f, Matrix3f, Vector2f, vec2f, vec3f};
use math::matrix::axis_angle::*;

use crate::camera::*;
use crate::solver::*;
use crate::extrinsics::*;


/// TODO: The PnP Solver is a special case of this with one camera with a fixed position.
pub struct BundleAdjustmentSolver<'a> {
    solver: NonLinearSolver<'a>,
    cameras: Vec<Camera<'a>>,
    objects: Vec<Object>
}

struct Camera<'a> {
    intrinsics: &'a CameraIntrinsicsModel,
    r_param: usize,
    t_param: usize,
    fixed: bool,
}

struct Object {
    r_param: usize,
    t_param: usize,
}

impl<'a> BundleAdjustmentSolver<'a> {

    pub fn new() -> Self {
        Self {
            solver: NonLinearSolver::new(),
            cameras: vec![],
            objects: vec![]
        }
    }

    pub fn add_camera(
        &mut self,
        intrinsics: &'a CameraIntrinsicsModel,
        initial_rotation: &Vector3f,
        initial_translation: &Vector3f,
        fixed: bool,
    ) -> usize {

        let idx = self.cameras.len();

        let r_param = self.solver.add_parameter_block(&[
            initial_rotation[0], initial_rotation[1], initial_rotation[2],
        ], AxisAngleParameterBlock::default());

        let t_param = self.solver.add_parameter_block(&[
            initial_translation[0], initial_translation[1], initial_translation[2]
        ], LinearParameterBlock::default());

        self.cameras.push(Camera {
            intrinsics,
            r_param,
            t_param,
            fixed
        });

        idx
    }

    pub fn add_object(
        &mut self,
        initial_rotation: &Vector3f,
        initial_translation: &Vector3f,
    ) -> usize {
        let idx = self.objects.len();

        let r_param = self.solver.add_parameter_block(&[
            initial_rotation[0], initial_rotation[1], initial_rotation[2],
        ], AxisAngleParameterBlock::default());

        let t_param = self.solver.add_parameter_block(&[
            initial_translation[0], initial_translation[1], initial_translation[2]
        ], LinearParameterBlock::default());

        self.objects.push(Object {
            r_param,
            t_param
        });

        idx
    }

    /// Adds an observation of a single point in an object by a single camera.
    pub fn add_object_point_view(
        &mut self,
        object_idx: usize,
        camera_idx: usize,
        point_2d: &Vector2f,
        point_3d: &Vector3f
    ) {

        let obj = &self.objects[object_idx];
        let cam = &self.cameras[camera_idx];

        self.solver.add_residual_block(
            &[ cam.r_param, cam.t_param, obj.r_param, obj.t_param ],
            ObjectPointReprojectionResidual {
                intrinsics: cam.intrinsics,
                point_2d: point_2d.clone(),
                point_3d: point_3d.clone(),
                fixed_camera: cam.fixed,
            }
        );
    }


    pub fn solve<'b>(&'b self) -> BundleAdjustmentSolution<'b, 'a> {
        BundleAdjustmentSolution {
            solver: self,
            solution: self.solver.solve()
        }
    }

}


pub struct BundleAdjustmentSolution<'a, 'b> {
    solver: &'a BundleAdjustmentSolver<'b>,
    solution: NonLinearProblemSolution<'a, 'b>,
}

impl<'a, 'b> BundleAdjustmentSolution<'a, 'b> {
    pub fn camera_extrinsics(&self, idx: usize) -> CameraExtrinsics {
        let cam = &self.solver.cameras[idx];

        CameraExtrinsics {
            rotation: Vector3f::from_slice(self.solution.param_block(cam.r_param)),
            translation: Vector3f::from_slice(self.solution.param_block(cam.t_param)),
        }
    }
}




/// Computes the reprojection error of a 3d point which is part of a
/// multi-point object into a single camera.
///
/// Params:
/// [0-5] : Camera rotation and translation
/// [6-11] : Object rotation and translation
struct ObjectPointReprojectionResidual<'a> {
    intrinsics: &'a CameraIntrinsicsModel,
    point_3d: Vector3f,
    point_2d: Vector2f,
    fixed_camera: bool,
}

impl<'a> ObjectPointReprojectionResidual<'a> {
    fn calc_error(
        &self,
        params: &[f32],
        camera_axis_angle: &Vector3f,
        object_axis_angle: &Vector3f,
    ) -> Vector2f {
        let camera_small_axis_angle = vec3f(params[0], params[1], params[2]);
        let camera_translation = vec3f(params[3], params[4], params[5]);

        let object_small_axis_angle = vec3f(params[6], params[7], params[8]);
        let object_translation = vec3f(params[9], params[10], params[11]);

        let pt3 = &self.point_3d;
        let pt2 = &self.point_2d;

        let projected = {
            let mut point = rotate_by_axis_angle(pt3, object_axis_angle);
            point = rotate_by_axis_angle_near_zero(&point, &object_small_axis_angle);
            point += object_translation;

            point = rotate_by_axis_angle(&point, camera_axis_angle);
            point = rotate_by_axis_angle_near_zero(&point, &camera_small_axis_angle);
            point += camera_translation;

            self.intrinsics.project_point(&point)
        };

        pt2 - projected
    }
}

impl<'a> ResidualBlockFunction for ObjectPointReprojectionResidual<'a> {
    fn len(&self) -> usize {
        2
    }

    fn calculate(&self, params: &[f32], out: &mut [f32], gradient: &mut [f32]) {
        let camera_axis_angle = vec3f(params[0], params[1], params[2]);
        let object_axis_angle = vec3f(params[6], params[7], params[8]);

        let mut expanded_params = params.to_vec();
        expanded_params[0..3].fill(0.0);
        expanded_params[6..9].fill(0.0);

        out.copy_from_slice(self.calc_error(&expanded_params, &camera_axis_angle, &object_axis_angle).as_ref());
 
        let step = 0.0001;
        for i in 0..params.len() {
            let v = expanded_params[i];
            expanded_params[i]  = v + step;

            let error1 = self.calc_error(&expanded_params, &camera_axis_angle, &object_axis_angle);

            expanded_params[i]  = v - step;
            let error2 = self.calc_error(&expanded_params, &camera_axis_angle, &object_axis_angle);

            expanded_params[i] = v;

            // negative since we want the gradient of project_point not the error function.
            let mut grad = (error1 - error2) / (-2.0 * step);

            if self.fixed_camera && i < 9 {
                grad = vec2f(0.0, 0.0);
            }

            gradient[i] = grad[0];
            gradient[params.len() + i] = grad[1];
        }
    }
}