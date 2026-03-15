use std::sync::Arc;
use std::time::{Duration, Instant};

use common::errors::*;
use math::matrix::VectorXd;
use math::vecxd;
use cnc_controller_proto::cnc::*;
use peripherals_proto::peripherals::StepperMotorMotion_Direction;
use cnc::quadratic_stepper_motion::QuadraticStepperMotion;
use terminal::TerminalTableBuilder;

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
    max_cycle_time: Duration,
    max_cycle_commands: usize,
    max_cycle_commands_per_motor: usize,
    linear_stats: LinearMotionStats,
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
            max_cycle_time: Duration::ZERO,
            max_cycle_commands: 0,
            max_cycle_commands_per_motor: 0,
            linear_stats: LinearMotionStats::default(),
        }
    }

    pub fn run(&mut self, cmds: &[Command]) -> Result<()> {

        for c in cmds {
            if c.has_move_to() {
                // TODO: KEep supproting x,y,z,e, fields
                let m = c.move_to();
                assert!(m.has_position());

                let options = MoveOptions::from_proto(m.options());

                self.planner.move_to_with_options(
                    VectorXd::from_proto(m.position()) - &self.position_offset,
                    &options
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

        // TODO: Pretty print.
        println!("Total Time: {}", base_units::format_duration_secs(Duration::from_secs_f64(self.total_time)));
        println!("Total # Motions: {}", self.total_motions);
        println!("Total # Steps: {}", self.total_steps);

        // 'cycle_interval - max_cycle_time' is the amount of time we have for sending commands.
        println!("Max Processing Time per Cycle: {:?}", self.max_cycle_time);

        // This must be smaller than the no-op limit and slammer than the stepper motor queue size (else we need to measure it over a sliding window)
        println!("Max Commands per Cycle: {:?} ({} per motor)", self.max_cycle_commands, self.max_cycle_commands_per_motor);

        self.linear_stats.print();

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

            // TODO: Perform test this.
            self.planner.next(planner_time, &mut out);
            for motion in out {
                self.linear_stats.add(&motion);
                queue.enqueue(motion);
            }
        }

        let mut motor_position = vec![0i64; 4];

        let mut i = STEP_GENERATION_STEP;
        while !queue.is_empty() {
            let s = Instant::now();

            let commands = queue.to_commands(i)?;

            let e = Instant::now();

            self.max_cycle_time = self.max_cycle_time.max(e - s);

            {
                let mut n = 0;
                for j in 0..commands.len() {
                    self.max_cycle_commands_per_motor = self.max_cycle_commands_per_motor.max(commands[j].len());

                    n += commands[j].len();
                }

                self.max_cycle_commands = self.max_cycle_commands.max(n);
            }


            for j in 0..commands.len() {
                self.total_motions += commands[j].len();

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

use std::collections::HashMap;

use cnc::linear_motion::LinearMotion;


#[derive(Default)]
pub struct LinearMotionStats {
    /// Amount of time spent in each category of 
    breakdown: HashMap<LinearMotionKey, f64>,
    limits: HashMap<Vec<usize>, f64>,
    total_time: f64,
}

#[derive(PartialEq, Eq, Clone, Hash)]
struct LinearMotionKey {
    // Bit map of which axes are moving in this motion.
    moving_axes: u8,
    accelerating: bool,
}

impl LinearMotionStats {

    pub fn add(&mut self, motion: &LinearMotion) {

        self.total_time += motion.duration;

        let dir = &motion.end_position - &motion.start_position;

        let mut moving_axes = 0;
        for i in 0..dir.len() {
            let moving = dir[i].abs() > 0.001;
            if moving {
                moving_axes |= 1 << i;
                
                // Treat x and y as the same group
                if i == 0 || i == 1 {
                    moving_axes |= 0b11;
                }
            }
        }


        let mut accelerating = false;
        for i in 0..motion.acceleration.len() {
            if motion.acceleration[i].abs() > 0.001 {
                accelerating = true;
            }
        }

        let key = LinearMotionKey {
            moving_axes,
            accelerating
        };

        *self.breakdown.entry(key).or_default() += motion.duration;

        if !accelerating {
            let mut speeds = vec![];

            let xy_speed = (squared(motion.start_velocity[0]) + squared(motion.start_velocity[1])).sqrt();
            speeds.push(xy_speed);

            for i in 2..motion.start_velocity.len() {
                speeds.push(motion.start_velocity[i].abs());
            }

            let mut filtered_speeds = vec![];

            for speed in speeds {
                let mut s = ((speed / 1.0).round() * 1.0) as usize;
                if speed > 0.01 && s == 0 {
                    s = 1;
                } 

                filtered_speeds.push(s);
            }

            *self.limits.entry(filtered_speeds).or_default() += motion.duration;
        }

    }

    pub fn print(&self) {

        println!("");
        println!("Time breakdown by axes:");

        let mut breakdown_values = self.breakdown.iter().collect::<Vec<_>>();
        breakdown_values.sort_by(|(_, t1), (_, t2)| t2.partial_cmp(t1).unwrap());


        let mut table1 = TerminalTableBuilder::new();
        table1.row().col("Axes Moving").col("Acceleration").col("Time Spent");

        for (key, time) in breakdown_values {

            let mut axes = String::new();

            for i in 0..4 {
                let c = {
                    if key.moving_axes & (1 << i) != 0 {
                        match i {
                            0 => 'X',
                            1 => 'Y',
                            2 => 'Z',
                            3 => 'E',
                            _ => panic!()
                        }
                    } else {
                        ' '
                    }
                };
                axes.push(c);
            }

            let accel = {
                if key.accelerating {
                    "ACCEL"
                } else {
                    "CONST"
                }
            };

            table1.row()
            .col(axes)
            .col(accel)
            .col(format!("{:.0} secs ({:.0}%)", time, 100.0 * (time / self.total_time)));
        }

        table1.print();

        let mut limit_values = self.limits.iter().collect::<Vec<_>>();
        limit_values.sort_by(|(_, t1), (_, t2)| t2.partial_cmp(t1).unwrap());




        // TODO: For this to be useful, I also need to know what the feedrate in the gcode is and what the machine limit is in our config 
        println!("");
        println!("Breakdown of Constant Speed Segments (by speed) (top 10 of {}):", limit_values.len());

        if limit_values.len() > 10 {
            limit_values.truncate(10);
        }

        let mut table2 = TerminalTableBuilder::new();
        table2.row().col("[XY, Z, E] speed").col("Time Spent");
        for (key, time) in limit_values {
            table2.row().col(format!("{:?}", key)).col(format!("{:.1}", time));
        }

        table2.print();

    }
}

fn squared(v: f64) -> f64 {
    v * v
}
