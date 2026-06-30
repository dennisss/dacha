use math::matrix::{VectorXd, MatrixXd, Vector3d, Matrix3d, Vector2d, vec2d, vec3d};
use math::matrix::axis_angle::*;

use crate::camera::*;
use crate::solver::*;
use crate::extrinsics::*;


/// TODO: The PnP Solver is a special case of this with one camera with a fixed position and intrinsics
pub struct BundleAdjustmentSolver {
    solver: NonLinearSolver<'static>,
    cameras: Vec<Camera>,
    objects: Vec<Object>
}

struct Camera {
    i_param: usize,
    r_param: usize,
    t_param: usize,
}

struct Object {
    r_param: usize,
    t_param: usize,
}

impl BundleAdjustmentSolver {

    pub fn new() -> Self {
        let mut solver = NonLinearSolver::new();

        Self {
            solver,
            cameras: vec![],
            objects: vec![]
        }
    }

    pub fn add_camera(
        &mut self,
        intrinsics: &CameraIntrinsicsModel,
        initial_rotation: &Vector3d,
        initial_translation: &Vector3d,
        fixed: bool,
    ) -> usize {

        let idx = self.cameras.len();

        let i_param = self.solver.add_parameter_block(
            &intrinsics.serialize(),
            LinearParameterBlock::default()
        );
        // self.solver.freeze_parameter_block(i_param);

        let r_param = self.solver.add_parameter_block(&[
            initial_rotation[0], initial_rotation[1], initial_rotation[2],
        ], AxisAngleParameterBlock::default());

        let t_param = self.solver.add_parameter_block(&[
            initial_translation[0], initial_translation[1], initial_translation[2]
        ], LinearParameterBlock::default());

        if fixed {
            self.solver.freeze_parameter_block(r_param);
            self.solver.freeze_parameter_block(t_param);
        }

        self.cameras.push(Camera {
            i_param,
            r_param,
            t_param,
        });

        idx
    }

    pub fn add_object(
        &mut self,
        initial_rotation: &Vector3d,
        initial_translation: &Vector3d,
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
        point_2d: &Vector2d,
        point_3d: &Vector3d
    ) {

        let obj = &self.objects[object_idx];
        let cam = &self.cameras[camera_idx];

        self.solver.add_residual_block(
            &[ cam.r_param, cam.t_param, obj.r_param, obj.t_param, cam.i_param ],
            ObjectPointReprojectionResidual {
                point_2d: point_2d.clone(),
                point_3d: point_3d.clone(),
            }
        );
    }


    pub fn solve<'b>(&'b self) -> BundleAdjustmentSolution<'b> {
        BundleAdjustmentSolution {
            solver: self,
            solution: self.solver.solve()
        }
    }

}


pub struct BundleAdjustmentSolution<'a> {
    solver: &'a BundleAdjustmentSolver,
    solution: NonLinearProblemSolution<'a, 'static>,
}

impl<'a> BundleAdjustmentSolution<'a> {
    pub fn camera_intrinsics(&self, idx: usize) -> CameraIntrinsicsModel {
        let cam = &self.solver.cameras[idx];

        CameraIntrinsicsModel::parse(self.solution.param_block(cam.i_param))
    }

    pub fn camera_extrinsics(&self, idx: usize) -> CameraExtrinsics {
        let cam = &self.solver.cameras[idx];

        CameraExtrinsics {
            rotation: Vector3d::from_slice(self.solution.param_block(cam.r_param)),
            translation: Vector3d::from_slice(self.solution.param_block(cam.t_param)),
        }
    }

    pub fn object_extrinsics(&self, idx: usize) -> CameraExtrinsics {
        let obj = &self.solver.objects[idx];

        CameraExtrinsics {
            rotation: Vector3d::from_slice(self.solution.param_block(obj.r_param)),
            translation: Vector3d::from_slice(self.solution.param_block(obj.t_param)),
        }
    }
}




/// Computes the reprojection error of a 3d point which is part of a
/// multi-point object into a single camera.
///
/// Params:
/// [0-5] : Camera rotation and translation
/// [6-11] : Object rotation and translation
/// [12..] : Camera intrinsics
struct ObjectPointReprojectionResidual {
    point_3d: Vector3d,
    point_2d: Vector2d,
}

impl ObjectPointReprojectionResidual {
    fn calc_error(
        &self,
        params: &[f64],
        camera_rotation: &Matrix3d,
        object_rotation: &Matrix3d,
    ) -> Vector2d {
        let camera_small_axis_angle = vec3d(params[0], params[1], params[2]);
        let camera_translation = vec3d(params[3], params[4], params[5]);

        let object_small_axis_angle = vec3d(params[6], params[7], params[8]);
        let object_translation = vec3d(params[9], params[10], params[11]);

        let intrinsics = CameraIntrinsicsModel::parse(&params[12..]);

        let pt3 = &self.point_3d;
        let pt2 = &self.point_2d;

        let projected = {
            let mut point = object_rotation * pt3;
            point = rotate_by_axis_angle_near_zero(&point, &object_small_axis_angle);
            point += object_translation;

            point = camera_rotation * &point;
            point = rotate_by_axis_angle_near_zero(&point, &camera_small_axis_angle);
            point += camera_translation;

            intrinsics.project_point(&point)
        };

        pt2 - projected
    }
}

impl ResidualBlockFunction for ObjectPointReprojectionResidual {
    fn len(&self) -> usize {
        2
    }

    fn calculate(&self, params: &[f64], out: &mut [f64], gradient: &mut [f64]) {
        let camera_rotation = from_axis_angle(&vec3d(params[0], params[1], params[2]));
        let object_rotation = from_axis_angle(&vec3d(params[6], params[7], params[8]));

        let mut expanded_params = params.to_vec();
        expanded_params[0..3].fill(0.0);
        expanded_params[6..9].fill(0.0);

        out.copy_from_slice(self.calc_error(&expanded_params, &camera_rotation, &object_rotation).as_ref());
 
        let step = 0.00001;
        for i in 0..params.len() {
            let v = expanded_params[i];
            expanded_params[i]  = v + step;

            let error1 = self.calc_error(&expanded_params, &camera_rotation, &object_rotation);

            expanded_params[i]  = v - step;
            let error2 = self.calc_error(&expanded_params, &camera_rotation, &object_rotation);

            expanded_params[i] = v;

            // negative since we want the gradient of project_point not the error function.
            let mut grad = (error1 - error2) / (-2.0 * step);
            gradient[i] = grad[0];
            gradient[params.len() + i] = grad[1];
        }
    }
}