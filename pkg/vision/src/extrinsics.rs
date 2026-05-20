use math::matrix::{Vector3f, Matrix3f, Matrix4f};
use math::matrix::axis_angle::*;

#[derive(Clone)]
pub struct CameraExtrinsics {
    pub rotation: Vector3f,
    pub translation: Vector3f,
}

impl CameraExtrinsics {

    pub fn to_mat4x4(&self) -> Matrix4f {
        let mut out = Matrix4f::zero();
        out.block_mut(0, 0).copy_from(&from_axis_angle(&self.rotation));
        out.block_mut(0, 3).copy_from(&self.translation);
        out
    }

    pub fn position(&self) -> Vector3f {
        (from_axis_angle(&self.rotation).inverse() * -1.0) * &self.translation
    }

}

fn cross_mat(v: &Vector3f) -> Matrix3f {
    Matrix3f::from_slice(&[
        0.0, -v[2], v[1],
        v[2], 0.0, -v[0],
        -v[1], v[0], 0.0
    ])
}

pub fn essential_matrix(cam1: &CameraExtrinsics, cam2: &CameraExtrinsics) -> Matrix3f {
    /*
    Essential matrix is defined as:
        cross_mat(translation_rel) * rotation_rel
    where the two '_rel' variables represent the relative move between the two cameras.

    Assuming the transform for camera 1,2 are
        T_1, T_2 (4x4 matrices),
    then the relative transform when converting from camera 1 to camera 2 is:
        T_2 (T_1)^-1
    */

    let r1 = from_axis_angle(&cam1.rotation);
    let r2 = from_axis_angle(&cam2.rotation);

    let t1 = &cam1.translation;
    let t2 = &cam2.translation;

    let r_rel = r2 * r1.transpose();
    let t_rel = t2 - &r_rel * t1;

    cross_mat(&t_rel) * r_rel
}