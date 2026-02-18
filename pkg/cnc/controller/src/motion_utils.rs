use std::collections::VecDeque;
use std::sync::Arc;

use common::errors::*;
use cnc_controller_proto::cnc::*;
use math::matrix::VectorXd;
use cnc::linear_motion::*;
use cnc::quadratic_stepper_motion::*;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorMotion_Direction};

use crate::time::DeviceTime;


pub fn to_motor_space(x: &VectorXd, config: &MotionControllerConfig) -> Vec<f64> {

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

    // Convert from mm to steps.
    for i in 0..x_motor.len() {
        x_motor[i] = x_motor[i] * config.motors()[i].steps_per_mm();
    }

    x_motor
}


pub fn from_motor_space(x: &[i32], config: &MotionControllerConfig) -> VectorXd {

    // Convert from steps to mm.
    let mut x_motor = vec![0.0; config.motors_len()];
    for i in 0..x_motor.len() {
        // TODO: Verify no loss with large step counts.
        x_motor[i] = (x[i] as f64) / config.motors()[i].steps_per_mm();
    }

    let mut x_pos = VectorXd::zero_with_shape(config.axes().len(), 1);

    for geometry in config.geometry() {

        match geometry.geometry_case() {
            AxisGeometryGeometryCase::Direct(v) => {
                x_pos[v.axis_index() as usize] = x_motor[v.motor_index() as usize];
            }
            AxisGeometryGeometryCase::CoreXy(v) => {
                let da = x_motor[v.a_motor_index() as usize];
                let db = x_motor[v.b_motor_index() as usize];
                
                x_pos[v.x_axis_index() as usize] = 0.5 * (da + db);
                x_pos[v.y_axis_index() as usize] = 0.5 * (da - db);
            }
            AxisGeometryGeometryCase::NOT_SET => {
                // return 
            }
            //
        }
    }

    x_pos
}

// TODO: Dedup this.
pub fn from_motor_space_f64(x: &[f64], config: &MotionControllerConfig) -> VectorXd {
    // Convert from steps to mm.
    let mut x_motor = vec![0.0; config.motors_len()];
    for i in 0..x_motor.len() {
        x_motor[i] = x[i] / (config.motors()[i].steps_per_mm() as f64);
    }

    let mut x_pos = VectorXd::zero_with_shape(config.axes().len(), 1);

    for geometry in config.geometry() {

        match geometry.geometry_case() {
            AxisGeometryGeometryCase::Direct(v) => {
                x_pos[v.axis_index() as usize] = x_motor[v.motor_index() as usize] as f64;
            }
            AxisGeometryGeometryCase::CoreXy(v) => {
                let da = x_motor[v.a_motor_index() as usize];
                let db = x_motor[v.b_motor_index() as usize];
                
                x_pos[v.x_axis_index() as usize] = (0.5 * (da + db)) as f64;
                x_pos[v.y_axis_index() as usize] = (0.5 * (da - db)) as f64;
            }
            AxisGeometryGeometryCase::NOT_SET => {
                // return 
            }
            //
        }
    }

    x_pos

}
