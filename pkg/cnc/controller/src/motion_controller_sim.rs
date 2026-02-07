use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use math::matrix::VectorXd;
use math::vecxd;
use cnc_controller_proto::cnc::*;
use peripherals_proto::peripherals::StepperMotorMotion_Direction;
use cnc::quadratic_stepper_motion::QuadraticStepperMotion;

use crate::devices::*;
use crate::config::*;
use crate::motion_controller::*;
use crate::endstop_controller::*;
use crate::machine_controller::MachineController;
use crate::stepper_motion_generator::StepperMotionGenerator;
use crate::time::DeviceTime;
use crate::proto_utils::VectorProtoExt;
use crate::motion_controller::{PLANNER_STEP_SIZE, STEP_GENERATION_STEP};


pub struct MotionControllerSimulator {
    config: Arc<MotionControllerConfig>,
    planner: MotionControllerLinearPlanner,
    position_offset: VectorXd,
    total_motions: usize,
    total_steps: usize,
    motor_positions: Vec<i32>,
    total_time: f64,
}

impl MotionControllerSimulator {

    pub fn new(config: Arc<MotionControllerConfig>, start_position: VectorXd) -> Self {
        let mut planner = MotionControllerLinearPlanner::new(config.clone());
        // planner.set_start_position(start_position.clone());

        Self {
            config: config.clone(),
            planner,
            position_offset: vecxd!(0.0, 0.0, 0.0, 0.0),
            motor_positions: vec![0, 0, 0, 0],
            total_motions: 0,
            total_steps: 0,
            total_time: 0.0,
        }
    }

    pub fn run(&mut self, cmds: &[Command]) -> Result<()> {

        for c in cmds {
            if c.has_move_to() {
                // TODO: KEep supproting x,y,z,e, fields
                let m = c.move_to();
                assert!(m.has_position());
                self.planner.move_to(
                    VectorXd::from_proto(m.position()) - &self.position_offset,
                    m.feed_rate()
                )?;
            }

            if c.has_set_position() {
                // println!("=== SET POSITION FLUSH!!");
                self.flush()?;

                let num_axes = 4;

                let p = c.set_position();
                self.position_offset = VectorXd::from_proto(p.position());

                self.motor_positions.clear();
                self.motor_positions.resize(4, 0);

                self.planner.set_start_position(VectorXd::zero_with_shape(num_axes, 1));

            }
        }

        // println!("Final flush!");
        self.flush()?;

        println!("Total motions: {}", self.total_motions);
        println!("Total Steps: {}", self.total_steps);

        println!("Total Time: {}", self.total_time);

        Ok(())

    }

    // This assumes that it can start at motor position zero.
    fn flush(&mut self) -> Result<()> {
        let mut remote_times = vec![];
        for i in 0..self.config.motors().len() {
            remote_times.push(DeviceTime::new_test_only(0, 16_000_000));
        }

        let mut queue = StepperMotionGenerator::new(self.config.clone(), &self.motor_positions, &remote_times);

        let last_position = self.planner.last_position().clone();

        let mut planner_time = 0.0;
        self.planner.set_start_time(planner_time);

        while !self.planner.is_empty() {
            planner_time += PLANNER_STEP_SIZE;

            let mut out = vec![];
            // TODO: Use the same constants as in the MotionController code.
            self.planner.next(planner_time, &mut out);
            for motion in out {
                queue.enqueue(motion);
            }
        }

        let mut motor_position = vec![0i64; 4];

        let mut i = STEP_GENERATION_STEP;
        while !queue.is_empty() {
            let commands = queue.to_commands(i)?;
            let mut n = vec![];
            for j in 0..commands.len() {
                self.total_motions += commands[j].len();
                n.push(commands[j].len());

                for (_, cmd) in &commands[j] {
                    let sign = if cmd.num_steps.direction() { 1 } else { -1 };

                    motor_position[j] += sign * (cmd.num_steps.count() as i64);
                    self.total_steps += cmd.num_steps.count() as usize;
                }

            }

            /*
            println!("{}: {:?}", i, n);
            println!("  pos: {:?}", queue.motor_positions());
            */
            i += STEP_GENERATION_STEP;
        }

        self.total_time += i;


        self.motor_positions.copy_from_slice(queue.motor_positions());
        

        // println!("Motor Pos: {:?}", motor_position);

        Ok(())
    }

}