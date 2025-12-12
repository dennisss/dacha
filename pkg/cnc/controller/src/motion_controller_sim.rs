use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use math::matrix::VectorXf;
use math::vecxf;
use cnc_controller_proto::cnc::*;
use cnc::linear_motion_planner::LinearMotionPlanner;
use peripherals_proto::peripherals::StepperMotorMotion_Direction;
use cnc::quadratic_stepper_motion::QuadraticStepperMotion;

use crate::devices::*;
use crate::config::*;
use crate::motion_controller::*;
use crate::gcode::CommandConverter;
use crate::endstop_controller::*;
use crate::machine_controller::MachineController;
use crate::stepper_motion_generator::StepperMotionGenerator;
use crate::time::DeviceTime;

pub struct MotionControllerSimulator {
    config: Arc<MotionControllerConfig>,
    planner: MotionControllerLinearPlanner,
    position_offset: VectorXf,
    total_motions: usize,
    total_steps: usize,
}

impl MotionControllerSimulator {

    pub fn new(config: Arc<MotionControllerConfig>) -> Self {
        Self {
            config: config.clone(),
            planner: MotionControllerLinearPlanner::new(config.clone()),
            position_offset: vecxf!(0.0, 0.0, 0.0, 0.0),
            total_motions: 0,
            total_steps: 0,
        }
    }

    pub fn run(&mut self, cmds: &[gcode::Command]) -> Result<()> {

        let mut converter = CommandConverter::new();

        for cmd in cmds {
            let mut out = vec![];
            converter.next(&cmd, &mut out)?;

            for c in out {
                if c.has_move_to() {
                    let m = c.move_to();
                    self.planner.move_to(
                        vecxf!(m.x(), m.y(), m.z(), m.e()) - &self.position_offset,
                        m.feed_rate()
                    )?;
                }

                if c.has_set_position() {
                    println!("=== SET POSITION FLUSH!!");
                    self.flush()?;

                    let num_axes = 4;

                    let p = c.set_position();
                    self.position_offset = vecxf!(p.x(), p.y(), p.z(), p.e());

                    self.planner.set_start_position(VectorXf::zero_with_shape(num_axes, 1));

                }

            }
        }

        println!("Final flush!");
        self.flush()?;

        println!("Total motions: {}", self.total_motions);
        println!("Total Steps: {}", self.total_steps);

        Ok(())

    }

    // This assumes that it can start at motor position zero.
    pub fn flush(&mut self) -> Result<()> {
        let mut remote_times = vec![];
        for i in 0..self.config.motors().len() {
            remote_times.push(DeviceTime::new_test_only(16_000_000));
        }

        let mut queue = StepperMotionGenerator::new(self.config.clone(), &[0, 0, 0, 0], &remote_times);

        let last_position = self.planner.last_position().clone();
        println!("Last Pos: {:?}", last_position);


        println!("Doing planning!");

        while !self.planner.is_empty() {
            let mut out = vec![];
            self.planner.next(1.0, 10000, &mut out);
            for motion in out {
                queue.enqueue(motion);
            }
        }

        println!("Doing steps!");

        // 283421.03

        // queue.motions.truncate(2);

        // println!("Have {} motions", queue.motions.len());

        let step = 10;

        let mut motor_position = vec![0i64; 4];

        let mut i = step;
        while !queue.is_empty() {
            let commands = queue.to_commands(i as f64)?;
            let mut n = vec![];
            for j in 0..commands.len() {
                self.total_motions += commands[j].len();
                n.push(commands[j].len());

                for cmd in &commands[j] {
                    let sign = if cmd.direction() == StepperMotorMotion_Direction::FORWARD { -1 } else { 1 };

                    motor_position[j] += sign * ((cmd.num_steps_minus_one() + 1) as i64);
                    self.total_steps += (cmd.num_steps_minus_one() + 1) as usize;
                }

            }

            println!("{}: {:?}", i, n);
            i += step;
        }

        

        println!("Motor Pos: {:?}", motor_position);

        assert!(((motor_position[3] as f64) / 708.365708) - (last_position[3] as f64) < 0.01);

        Ok(())
    }

}