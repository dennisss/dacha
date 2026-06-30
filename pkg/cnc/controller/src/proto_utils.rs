use common::errors::*;
use math::matrix::{VectorXd, MatrixXd};
use math::vecxd;
use cnc_controller_proto::cnc::LinearMotionProto;
use cnc::linear_motion::LinearMotion;
use math_proto_util::VectorProtoExt;

pub trait LinearMotionProtoExt {
    fn from_proto(p: &LinearMotionProto) -> Result<Self> where Self: Sized;

    fn to_proto(&self) -> LinearMotionProto;
}

impl LinearMotionProtoExt for LinearMotion {
    fn from_proto(p: &LinearMotionProto) -> Result<Self> {
        Ok(Self {
            start_position: VectorXd::from_proto(p.start_position())?,
            start_velocity: VectorXd::from_proto(p.start_velocity())?,
            acceleration: VectorXd::from_proto(p.acceleration())?,
            duration: p.duration(),

            // TODO
            end_position: vecxd!(),
            end_velocity: vecxd!(),
        })
    }
    
    fn to_proto(&self) -> LinearMotionProto {
        let mut out = LinearMotionProto::default();
        out.set_start_position(self.start_position.to_proto());
        out.set_start_velocity(self.start_velocity.to_proto());
        out.set_acceleration(self.acceleration.to_proto());
        out.set_duration(self.duration);
        out
    }
}
