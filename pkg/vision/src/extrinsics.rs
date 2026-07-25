use math::matrix::{Vector3d, Matrix3d, Matrix4d};
use math::matrix::axis_angle::*;
use vision_proto::vision::CameraExtrinsicsProto;

#[derive(Clone, Default, Debug)]
pub struct CameraExtrinsics {
    pub rotation: Vector3d,
    pub translation: Vector3d,
}

impl CameraExtrinsics {

    pub fn to_mat4x4(&self) -> Matrix4d {
        let mut out = Matrix4d::zero();
        out.block_mut(0, 0).copy_from(&from_axis_angle(&self.rotation));
        out.block_mut(0, 3).copy_from(&self.translation);
        out[(3, 3)] = 1.0;
        out
    }

    pub fn from_mat4x4(mat: &Matrix4d) -> Self {
        Self {
            rotation: to_axis_angle(&mat.block(0, 0).to_owned()),
            translation: mat.block(0, 3).to_owned()
        }
    }

    pub fn position(&self) -> Vector3d {
        (from_axis_angle(&self.rotation).inverse().unwrap() * -1.0) * &self.translation
    }

    pub fn transform(&self, pt: &Vector3d) -> Vector3d {
        rotate_by_axis_angle(pt, &self.rotation) + &self.translation
    }

    pub fn to_proto(&self) -> CameraExtrinsicsProto {
        let mut proto = CameraExtrinsicsProto::default();
        proto.rotation_mut().extend_from_slice(self.rotation.as_ref());
        proto.translation_mut().extend_from_slice(self.translation.as_ref());
        proto
    }

    pub fn from_proto(proto: &CameraExtrinsicsProto) -> Self {
        // TODO: Bounds checks.
        Self {
            rotation: Vector3d::from_slice(proto.rotation()),
            translation: Vector3d::from_slice(proto.translation()),
        }
    }
}

fn cross_mat(v: &Vector3d) -> Matrix3d {
    Matrix3d::from_slice(&[
        0.0, -v[2], v[1],
        v[2], 0.0, -v[0],
        -v[1], v[0], 0.0
    ])
}

pub fn essential_matrix(cam1: &CameraExtrinsics, cam2: &CameraExtrinsics) -> Matrix3d {
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