use math::matrix::VectorXf;
use cnc_controller_proto::cnc::VectorProto;

pub trait VectorProtoExt {
    fn from_proto(p: &VectorProto) -> Self;

    fn to_proto(&self) -> VectorProto;
}

impl VectorProtoExt for VectorXf {
    fn from_proto(p: &VectorProto) -> Self {
        VectorXf::from_slice_with_shape(p.values().len(), 1, p.values())
    }

    fn to_proto(&self) -> VectorProto {
        let mut out = VectorProto::default();
        for i in 0..self.len() {
            out.add_values(self[i]);
        }
        out
    }
}