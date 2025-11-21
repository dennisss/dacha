use std::collections::VecDeque;
use std::sync::Arc;

use common::errors::*;
use cnc_controller_proto::cnc::*;
use math::matrix::Vector3f;
use cnc::linear_motion::*;
use cnc::quadratic_stepper_motion::*;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorMotion_Direction};

use crate::time::DeviceTime;


pub fn to_motor_space(x: &Vector3f, config: &MotionControllerConfig) -> Vec<f32> {

    let mut x_motor = vec![0.0; config.motors_len()];

    // TODO: There is an assumption here that linear interolation of motor step positions translates to linear motion in the XYZ space.
    for geometry in config.geometry() {

        match geometry.geometry_case() {
            AxisGeometryGeometryCase::Direct(v) => {
                x_motor[v.motor_index() as usize] =
                    x[v.axis_index() as usize];
            }
            AxisGeometryGeometryCase::CoreXy(v) => {
                let dx = x[v.x_axis_index() as usize];
                let dy = x[v.y_axis_index() as usize];

                x_motor[v.a_motor_index() as usize] = dx + dy;
                x_motor[v.b_motor_index() as usize] = dx - dy;
            }
            AxisGeometryGeometryCase::NOT_SET => {
                // return 
            }
            //
        }
    }

    for i in 0..x_motor.len() {
        x_motor[i] = x_motor[i] * config.motors()[i].steps_per_mm();
    }

    x_motor
}


