use math::matrix::{VectorXd, MatrixXd, Vector3d, Matrix3d, Vector2d, vec2d, vec3d};
use math::matrix::axis_angle::*;

use crate::camera::*;
use crate::solver::*;
use crate::extrinsics::*;


/// TODO: The PnP Solver is a special case of this with one camera with a fixed position and intrinsics
pub struct BundleAdjustmentSolver {
    solver: NonLinearSolver<'static>,
    cameras: Vec<Camera>,
    objects: Vec<Object>,
    shared_points: Vec<SharedPoint>,

    // These are just used for computing RMS error at the end.
    object_points: Vec<ObjectPoint>
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

/// A point in an object's local coordinate frame whose position is optimized.
///
/// It can be shared by observations of many objects (e.g. the same marker on a
/// calibration wand seen at many different wand poses).
struct SharedPoint {
    /// One single-value parameter block per axis so that each axis can be
    /// independently frozen.
    params: [usize; 3],
}

enum ObjectPointSource {
    Fixed(Vector3d),
    Shared(usize),
}

struct ObjectPoint {
    camera_idx: usize,
    object_idx: usize,
    point: ObjectPointSource,
    point_2d: Vector2d,
}

/// Reprojection residual of one observation of an object point by a camera.
#[derive(Clone, Debug)]
pub struct ObjectPointViewResidual {
    pub camera_idx: usize,
    pub object_idx: usize,

    /// If this was added with add_shared_object_point_view, the index of the
    /// shared point.
    pub shared_point_idx: Option<usize>,

    /// Position of the point in the object's local frame (for shared points,
    /// this is the solved position).
    pub point_3d: Vector3d,

    /// Observed point minus the projected point (in pixels).
    pub residual: Vector2d,
}

impl BundleAdjustmentSolver {

    pub fn new() -> Self {
        let mut solver = NonLinearSolver::new();

        Self {
            solver,
            cameras: vec![],
            objects: vec![],
            shared_points: vec![],
            object_points: vec![]
        }
    }

    pub fn set_max_iterations(&mut self, n: usize) {
        self.solver.set_max_iterations(n);
    }

    pub fn set_use_gradient_descent(&mut self) {
        self.solver.set_use_gradient_descent();
    }

    pub fn enable_logging(&mut self) {
        self.solver.enable_logging();
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

    pub fn freeze_camera_intrinsics(&mut self, camera_idx: usize) {
        self.solver.freeze_parameter_block(self.cameras[camera_idx].i_param);
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
                point_3d: Some(point_3d.clone()),
            }
        );

        self.object_points.push(ObjectPoint {
            camera_idx,
            object_idx,
            point: ObjectPointSource::Fixed(point_3d.clone()),
            point_2d: point_2d.clone(),
        });
    }

    /// Adds a point (in object local coordinates) whose position will be
    /// optimized along with the rest of the parameters.
    ///
    /// 'free_axes' selects which of the x/y/z coordinates may change. The rest
    /// stay at their initial values (useful for fixing the gauge freedom of
    /// the object's coordinate frame).
    pub fn add_shared_object_point(
        &mut self,
        initial_point: &Vector3d,
        free_axes: [bool; 3],
    ) -> usize {
        let idx = self.shared_points.len();

        let mut params = [0; 3];
        for i in 0..3 {
            params[i] = self.solver.add_parameter_block(
                &[initial_point[i]], LinearParameterBlock::default());
            if !free_axes[i] {
                self.solver.freeze_parameter_block(params[i]);
            }
        }

        self.shared_points.push(SharedPoint { params });

        idx
    }

    /// Like add_object_point_view but the 3d point is a shared point created
    /// with add_shared_object_point.
    pub fn add_shared_object_point_view(
        &mut self,
        object_idx: usize,
        camera_idx: usize,
        point_2d: &Vector2d,
        shared_point_idx: usize,
    ) {
        let obj = &self.objects[object_idx];
        let cam = &self.cameras[camera_idx];
        let pt = &self.shared_points[shared_point_idx];

        self.solver.add_residual_block(
            &[
                cam.r_param, cam.t_param, obj.r_param, obj.t_param,
                pt.params[0], pt.params[1], pt.params[2],
                cam.i_param
            ],
            ObjectPointReprojectionResidual {
                point_2d: point_2d.clone(),
                point_3d: None,
            }
        );

        self.object_points.push(ObjectPoint {
            camera_idx,
            object_idx,
            point: ObjectPointSource::Shared(shared_point_idx),
            point_2d: point_2d.clone(),
        });
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

    pub fn shared_object_point(&self, idx: usize) -> Vector3d {
        let pt = &self.solver.shared_points[idx];
        let mut out = Vector3d::zero();
        for i in 0..3 {
            out[i] = self.solution.param_block(pt.params[i])[0];
        }
        out
    }

    /// Projects a point given in an object's local frame into a camera's image
    /// using the solved parameters.
    pub fn project_object_point(
        &self, camera_idx: usize, object_idx: usize, point_3d: &Vector3d
    ) -> Vector2d {
        let cam_int = self.camera_intrinsics(camera_idx);
        let cam_ext = self.camera_extrinsics(camera_idx);
        let obj_ext = self.object_extrinsics(object_idx);

        cam_int.project_point(&cam_ext.transform(&obj_ext.transform(point_3d)))
    }

    /// Computes the reprojection residual of every point view in the same order
    /// in which they were added.
    pub fn object_point_view_residuals(&self) -> Vec<ObjectPointViewResidual> {
        let mut out = vec![];

        for pt in &self.solver.object_points {
            let (point_3d, shared_point_idx) = match &pt.point {
                ObjectPointSource::Fixed(p) => (p.clone(), None),
                ObjectPointSource::Shared(idx) => (self.shared_object_point(*idx), Some(*idx)),
            };

            let expected = self.project_object_point(pt.camera_idx, pt.object_idx, &point_3d);

            out.push(ObjectPointViewResidual {
                camera_idx: pt.camera_idx,
                object_idx: pt.object_idx,
                shared_point_idx,
                point_3d,
                residual: pt.point_2d.clone() - expected,
            });
        }

        out
    }

    /// Calculates the final RMS reprojection error.
    pub fn error(&self) -> f64 {
        let mut sum = 0.0;
        let mut n = 0;

        for r in self.object_point_view_residuals() {
            sum += r.residual.norm_squared();
            n += 1;
        }

        let rms = (sum / (n as f64)).sqrt();
        rms
    }

}




/// Computes the reprojection error of a 3d point which is part of a
/// multi-point object into a single camera.
///
/// Params:
/// [0-5] : Camera rotation and translation
/// [6-11] : Object rotation and translation
/// [12-14] : Object local point (only present if 'point_3d' is None)
/// [12.. or 15..] : Camera intrinsics
struct ObjectPointReprojectionResidual {
    point_3d: Option<Vector3d>,
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

        let (pt3, intrinsics) = match &self.point_3d {
            Some(p) => (p.clone(), CameraIntrinsicsModel::parse(&params[12..])),
            None => (
                vec3d(params[12], params[13], params[14]),
                CameraIntrinsicsModel::parse(&params[15..])
            ),
        };
        let pt3 = &pt3;
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