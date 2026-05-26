use math::matrix::{Vector2f, Vector3f, vec2f, Matrix3f};
use math::matrix::cwise_binary_ops::{CwiseMulAssign, CwiseDivAssign};

use crate::solver::ParameterBlockOperator;

#[derive(Clone, Debug)]
pub struct CameraIntrinsicsModel {
    pub focal_length: Vector2f,

    pub center: Vector2f,

    pub k1: f32,

    pub k2: f32,
}

impl CameraIntrinsicsModel {
    /// 'focal_length' and 'pixel_size' should be both be given in the same units. 
    pub fn from_nominal_params(
        frame_width: usize,
        frame_height: usize,
        focal_length: f32,
        pixel_size: f32
    ) -> Self {
        let center = vec2f((frame_width as f32) / 2.0, (frame_height as f32) / 2.0);
        let focal_length = focal_length / pixel_size;

        Self {
            focal_length: vec2f(focal_length, focal_length),
            center,
            k1: 0.,
            k2: 0.,
        }
    }

    /// Gets the matrix for the focal lengths and camera center
    pub fn mat3(&self) -> Matrix3f {
        let mut out = Matrix3f::default();
        out[(0, 0)] = self.focal_length[0];
        out[(1, 1)] = self.focal_length[1];
        out[(2, 0)] = self.center[0];
        out[(2, 1)] = self.center[1];
        out[(2, 2)] = 1.0;
        out
    }

    /// Projects a 3d point (which is already translated so that camera is at 0,0,0)
    /// into a 2d point on the camera's image.
    pub fn project_point(&self, point: &Vector3f) -> Vector2f {
        let mut point_2d = vec2f(point[0] / point[2], point[1] / point[2]);

        point_2d *= self.calculate_distortion(&point_2d);

        point_2d.cwise_mul_assign(&self.focal_length);
        point_2d += &self.center;

        point_2d
    }

    fn calculate_distortion(&self, point: &Vector2f) -> f32 {
        let r2 = point.norm_squared();
        let r4 = r2*r2;
        1.0 + self.k1 * r2 + self.k2 * r4
    }

    pub fn unproject_point(&self, point: &Vector2f) -> Vector2f {
        let mut point = point.clone();
        point -= &self.center;
        point.cwise_div_assign(&self.focal_length);
        point = self.undistort(&point);
        point
    }

    fn undistort(&self, point: &Vector2f) -> Vector2f {
        let mut pt = point.clone();

        // TODO: Provide better convergence guarantees.
        for _ in 0..5 {
            pt = point.clone() / self.calculate_distortion(&pt);
        }

        pt
    }

    pub fn parse(values: &[f32]) -> Self {
        assert_eq!(values.len(), 6);

        Self {
            focal_length: vec2f(values[0], values[1]),
            center: vec2f(values[2], values[3]),
            k1: values[4],
            k2: values[5],
        }
    }

    pub fn serialize(&self) -> Vec<f32> {
        let mut out = vec![];
        out.extend_from_slice(self.focal_length.as_ref());
        out.extend_from_slice(self.center.as_ref());
        out.push(self.k1);
        out.push(self.k2);
        out
    }

}

pub fn millis(v: f32) -> f32 {
    v / 1_000.0
}

pub fn micros(v: f32) -> f32 {
    v / 1_000_000.0
}

pub struct CameraIntrinsicsParameterBlock {
    min: Vec<f32>,
    max: Vec<f32>
}

impl CameraIntrinsicsParameterBlock {
    // TODO: Make the threshold configurable
    pub fn new(initial_guess: CameraIntrinsicsModel) -> Self {
        let max_error = CameraIntrinsicsModel {
            focal_length: initial_guess.focal_length.clone() * 0.25,
            center: initial_guess.center.clone() * 0.25,
            k1: 0.5,
            k2: 0.5
        }.serialize();

        let mut max = initial_guess.serialize();
        let mut min = max.clone();

        for i in 0..max_error.len() {
            max[i] += max_error[i];
            min[i] -= max_error[i];
        }

        Self { min, max }
    }
}

impl ParameterBlockOperator for CameraIntrinsicsParameterBlock {
    fn update(&self, step: &[f32], params: &mut [f32]) {
        for i in 0..step.len() {
            params[i] = (params[i] + step[i]).max(self.min[i]).min(self.max[i]);
        }
    }
}
