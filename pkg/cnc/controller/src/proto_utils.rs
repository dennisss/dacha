use math::matrix::{VectorXd, MatrixXd};
use cnc_controller_proto::cnc::{VectorProto, MatrixProto};

pub trait VectorProtoExt {
    fn from_proto(p: &VectorProto) -> Self;

    fn to_proto(&self) -> VectorProto;
}

impl VectorProtoExt for VectorXd {
    fn from_proto(p: &VectorProto) -> Self {
        VectorXd::from_slice_with_shape(p.values().len(), 1, p.values())
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
    fn from_proto(p: &MatrixProto) -> Self;

    fn to_proto(&self) -> MatrixProto;
}

impl MatrixProtoExt for MatrixXd {
    // TODO: Need size validation
    fn from_proto(p: &MatrixProto) -> Self {
        MatrixXd::from_slice_with_shape(p.rows() as usize, p.cols() as usize, p.values())
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