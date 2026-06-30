use common::errors::*;
use math::matrix::{VectorXd, Vector3d, MatrixXd};
use math::vecxd;
use math_proto::math::*;

pub trait VectorProtoExt {
    fn from_proto(p: &VectorProto) -> Result<Self> where Self: Sized;

    fn to_proto(&self) -> VectorProto;
}

impl VectorProtoExt for VectorXd {
    fn from_proto(p: &VectorProto) -> Result<Self> {
        Ok(VectorXd::from_slice_with_shape(p.values().len(), 1, p.values()))
    }

    fn to_proto(&self) -> VectorProto {
        let mut out = VectorProto::default();
        for i in 0..self.len() {
            out.add_values(self[i]);
        }
        out
    }
}

impl VectorProtoExt for Vector3d {
    fn from_proto(p: &VectorProto) -> Result<Self> {
        if p.values().len() != 3 {
            return Err(err_msg("Unexpected number of entries for Vector3d"));
        }

        Ok(Vector3d::from_slice_with_shape(p.values().len(), 1, p.values()))
    }

    fn to_proto(&self) -> VectorProto {
        let mut out = VectorProto::default();
        for i in 0..self.len() {
            out.add_values(self[i]);
        }
        out
    }
}

pub trait MatrixProtoExt {
    fn from_proto(p: &MatrixProto) -> Result<Self> where Self: Sized;

    fn to_proto(&self) -> MatrixProto;
}

impl MatrixProtoExt for MatrixXd {
    fn from_proto(p: &MatrixProto) -> Result<Self> {
        if p.values().len() != (p.rows() * p.cols()) as usize {
            return Err(err_msg("Incorrect number of values in MatrixProto"));
        }

        Ok(MatrixXd::from_slice_with_shape(p.rows() as usize, p.cols() as usize, p.values()))
    }

    fn to_proto(&self) -> MatrixProto {
        let mut out = MatrixProto::default();
        for i in 0..self.len() {
            out.add_values(self[i]);
        }
        out.set_rows(self.rows() as u32);
        out.set_cols(self.cols() as u32);

        out
    }
}
