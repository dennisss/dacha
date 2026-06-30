use math::matrix::{Vector2d, Vector3d, vec2d, Matrix3d};
use math::matrix::cwise_binary_ops::{CwiseMulAssign, CwiseDivAssign};
use vision_proto::vision::CameraIntrinsicsModelProto;

use crate::solver::ParameterBlockOperator;

#[derive(Clone, Debug)]
pub struct CameraIntrinsicsModel {
    pub focal_length: Vector2d,

    pub center: Vector2d,

    pub k1: f64,

    pub k2: f64,
}

impl CameraIntrinsicsModel {
    /// 'focal_length' and 'pixel_size' should be both be given in the same units. 
    pub fn from_nominal_params(
        frame_width: usize,
        frame_height: usize,
        focal_length: f64,
        pixel_size: f64
    ) -> Self {
        let center = vec2d((frame_width as f64) / 2.0, (frame_height as f64) / 2.0);
        let focal_length = focal_length / pixel_size;

        Self {
            focal_length: vec2d(focal_length, focal_length),
            center,
            k1: 0.,
            k2: 0.,
        }
    }

    /// Gets the matrix for the focal lengths and camera center
    pub fn mat3(&self) -> Matrix3d {
        let mut out = Matrix3d::default();
        out[(0, 0)] = self.focal_length[0];
        out[(1, 1)] = self.focal_length[1];
        out[(2, 0)] = self.center[0];
        out[(2, 1)] = self.center[1];
        out[(2, 2)] = 1.0;
        out
    }

    /// Projects a 3d point (which is already translated so that camera is at 0,0,0)
    /// into a 2d point on the camera's image.
    pub fn project_point(&self, point: &Vector3d) -> Vector2d {
        let z_inv = 1.0 / point[2].max(0.001);
        let mut point_2d = vec2d(point[0] * z_inv, point[1] * z_inv);

        point_2d *= self.calculate_distortion(&point_2d);

        point_2d.cwise_mul_assign(&self.focal_length);
        point_2d += &self.center;

        point_2d
    }

    fn calculate_distortion(&self, point: &Vector2d) -> f64 {
        let r2 = point.norm_squared();
        let r4 = r2*r2;
        1.0 + self.k1 * r2 + self.k2 * r4
    }

    pub fn unproject_point(&self, point: &Vector2d) -> Vector2d {
        let mut point = point.clone();
        point -= &self.center;
        point.cwise_div_assign(&self.focal_length);
        point = self.undistort(&point);
        point
    }

    fn undistort(&self, point: &Vector2d) -> Vector2d {
        let mut pt = point.clone();

        // TODO: Provide better convergence guarantees.
        for _ in 0..5 {
            pt = point.clone() / self.calculate_distortion(&pt);
        }

        pt
    }

    pub fn parse(values: &[f64]) -> Self {
        assert_eq!(values.len(), 6);

        Self {
            focal_length: vec2d(values[0], values[1]),
            center: vec2d(values[2], values[3]),
            k1: values[4],
            k2: values[5],
        }
    }

    pub fn serialize(&self) -> Vec<f64> {
        let mut out = vec![];
        out.extend_from_slice(self.focal_length.as_ref());
        out.extend_from_slice(self.center.as_ref());
        out.push(self.k1);
        out.push(self.k2);
        out
    }

    pub fn from_proto(proto: &CameraIntrinsicsModelProto) -> Self {
        // TODO: Bounds checks.

        Self {
            focal_length: Vector2d::from_slice(proto.focal_length()),
            center: Vector2d::from_slice(proto.center()),
            k1: proto.k1(),
            k2: proto.k2()
        }
    }


}

pub fn millis(v: f64) -> f64 {
    v / 1_000.0
}

pub fn micros(v: f64) -> f64 {
    v / 1_000_000.0
}

