use std::{collections::HashMap, sync::Arc, time::Instant};
use std::collections::VecDeque;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

use base_error::*;
use peripherals_proto::peripherals::PeripheralRequest;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorMotion_Direction, StepperMotorStatus, StepperMotorStatus_StoppedReason};
use cnc_controller_proto::cnc::*;
use cnc::quadratic_stepper_motion::*;
use cnc::linear_motion::LinearMotion;
use cnc_controller::proto_utils::*;
use math::matrix::{VectorXd, MatrixXd};
use math::vecxd;

// TODO: Ideally have a more standardized way of doing this so that we can also use it
// on the controller side for stuff like hit position lookups.
pub struct MotionLog {
    pub start_time: u64,
    pub start_position: VectorXd,
    pub start_motor_position: Vec<i32>,

    pub end_time: u64,
    pub end_motor_position: Vec<i32>,
    
    stepper_motions: Vec<Vec<StepperMotionEntry>>,
    linear_motions: Vec<LinearMotionEntry>,
}

#[derive(Clone)]
struct StepperMotionEntry {
    start_time: u64,
    start_motor_position: i32,
    motion: QuadraticStepperMotion,
}

struct LinearMotionEntry {
    start_time: u64,
    motion: LinearMotion,
}

impl MotionLog {
    pub fn create(entries: &[LogEntry]) -> Result<Self> {

        let mut saw_motion_start = false;
        let mut saw_motion_end = false;
        let mut start_time = 0;
        let mut start_position = vecxd!();
        let mut start_motor_position = vec![];
        let mut end_time = 0;
        let mut end_position = vecxd!();
        let mut end_motor_position = vec![];
        let mut stepper_motions = vec![];
        let mut linear_motions = vec![];

        let mut current_motor_position = vec![];

        let mut last_directions = vec![];

        for entry in entries {
            if entry.has_motion_start() {
                if saw_motion_start {
                    return Err(err_msg("Multiple motion starts"));
                }

                saw_motion_start = true;

                start_time = entry.motion_start().time();
                start_motor_position.clear();
                start_motor_position.extend_from_slice(entry.motion_start().motor_position());
                start_position = VectorXd::from_proto(entry.motion_start().position());

                current_motor_position = start_motor_position.clone();
                last_directions.resize(current_motor_position.len(), StepperMotorMotion_Direction::UNCHANGED);
                stepper_motions.resize(current_motor_position.len(), vec![]);

                continue;
            }

            if entry.has_motion_end() {
                if !saw_motion_start {
                    return Err(err_msg("Saw motion end but no motion start"));
                }

                if saw_motion_end {
                    return Err(err_msg("Multiple motion ends"));
                }

                saw_motion_end = true;

                end_time = entry.motion_end().time();
                end_motor_position.clear();
                end_motor_position.extend_from_slice(entry.motion_end().motor_position());
                end_position = VectorXd::from_proto(entry.motion_end().position());

                // Verifying that if we trace through all individual stepper motions, we get to the
                // same position as reported by the controller.
                if end_motor_position != current_motor_position {
                    return Err(err_msg("Did not recover the same end motor position"));
                }

                continue;
            }

            if entry.has_stepper_motions() {
                if !saw_motion_start {
                    return Err(err_msg("Stepper motions before start of motion"));
                }
                if saw_motion_end {
                    return Err(err_msg("Stepper motions after end of motion"));
                }

                for motion in entry.stepper_motions().motions() {

                    let motor_index = motion.motor_index() as usize;

                    let mut dir_proto = motion.motion().direction();
                    if dir_proto == StepperMotorMotion_Direction::UNCHANGED {
                        if last_directions[motor_index] == StepperMotorMotion_Direction::UNCHANGED {
                            return Err(err_msg("Unknown first direction"));
                        }

                        dir_proto = last_directions[motor_index];
                    }

                    last_directions[motor_index] = dir_proto;

                    let direction = match dir_proto {
                        StepperMotorMotion_Direction::UNCHANGED => panic!(),
                        StepperMotorMotion_Direction::FORWARD => true,
                        StepperMotorMotion_Direction::BACKWARD => false
                    };

                    let inst = QuadraticStepperMotion {
                        next_step_time: motion.motion().next_step_time(),
                        next_step_duration: motion.motion().next_step_duration(),
                        step_duration_increment: motion.motion().step_duration_increment(),
                        num_steps: StepCount::new(motion.motion().num_steps_minus_one() + 1, direction),
                    };

                    // TODO: Verify that all start_times are monotonic.

                    stepper_motions[motor_index].push(StepperMotionEntry {
                        start_time: motion.start_time(),
                        start_motor_position: current_motor_position[motor_index],
                        motion: inst.clone(),
                    });

                    current_motor_position[motor_index] += inst.num_steps.delta(); 
                }
            }

            if entry.has_linear_motions() {
                if !saw_motion_start {
                    return Err(err_msg("Stepper motions before start of motion"));
                }
                if saw_motion_end {
                    return Err(err_msg("Stepper motions after end of motion"));
                }

                for proto in entry.linear_motions().motions() {
                    linear_motions.push(LinearMotionEntry {
                        start_time: proto.time(),
                        motion: LinearMotion::from_proto(proto.motion()),
                    });
                }
            }
        }

        if !saw_motion_end {
            return Err(err_msg("Did not see a complete motion in the log"));
        }

        // Fine tune the end time based on the last linear motion.
        // The end_motion even happens after a timeout after the last motion so is somewhat coarse.
        if let Some(entry) = linear_motions.last() {
            end_time = end_time.min(entry.start_time + ((entry.motion.duration * 16_000_000.0).ceil() as u64));
        }

        Ok(Self {
            start_time,
            start_position,
            start_motor_position,
            end_time,
            end_motor_position,
            stepper_motions,
            linear_motions,
        })
    }

    pub fn motor_positions_at_time(&self, time: u64) -> Option<Vec<f64>> {
        if time < self.start_time || time >= self.end_time {
            return None;
        }

        let mut out = vec![];

        for motor_i in 0..self.start_motor_position.len() {
            
            let motions = &self.stepper_motions[motor_i];

            if motions.len() == 0 || time <= motions[0].start_time {
                out.push(self.start_motor_position[motor_i] as f64);
                continue;
            }

            if time == self.end_time {
                out.push(self.end_motor_position[motor_i] as f64);
                continue;
            }


            let mut motion_found = false;
            for motion_i in 0..motions.len() {

                let start_time = motions[motion_i].start_time;
                let end_time = {
                    if motion_i + 1 < motions.len() {
                        motions[motion_i + 1].start_time
                    } else {
                        self.end_time
                    }
                };

                // Finding the motion containing the requested time.
                // TODO: We can binary search this.
                let good = time >= start_time && time < end_time;
                if !good {
                    continue;
                }

                motion_found = true;


                let mut cur_position = motions[motion_i].start_motor_position as f64;
                let mut step_start_time = start_time;
                let mut stepper = motions[motion_i].motion.clone();

                let sign: f64 = if stepper.num_steps.direction() { 1.0 } else { -1.0 };

                let mut found_step = false;
                while stepper.num_steps.count() > 0 {                
                    let mut step_dur = stepper.next_step_duration as u64;
                    let step_end_time = {   
                        if stepper.num_steps.count() > 1 {
                            step_start_time + (step_dur as u64)
                        } else {
                            step_dur = end_time - step_start_time;
                            
                            // TODO: If the next motion is not immediately following the current one
                            // then this may not be a good estimate. Ideally we pull more reach time
                            // information from the controller.
                            end_time
                        }
                    };

                    stepper.next();

                    if step_end_time > time {
                        found_step = true;

                        let delta = ((time - step_start_time) as f64) / (step_dur as f64);
                        cur_position += delta * sign;

                        break;
                    }

                    step_start_time = step_end_time;
                    cur_position += sign;

                }

                assert!(found_step);

                out.push(cur_position);


                break;
            }

            assert!(motion_found);
        }

        Some(out)
    }

    pub fn check_hit_speed(&self, speed: f64) -> Result<()> {
        let mut duration = 0.0;

        // Sum up duration of all constant velocity segments at the given speed.
        for entry in &self.linear_motions {
            if entry.motion.acceleration.norm() > 0.01 {
                continue;
            }

            let hit_speed = entry.motion.start_velocity.norm();
            if (speed - hit_speed).abs() < 0.1 {
                duration += entry.motion.duration;
            }
        }

        if duration < 0.1 {
            return Err(format_err!("Did not run at speed {} for at least 250ms. Actual duration: {}", speed, duration))
        }

        Ok(())
    }

    pub fn position_derivatives_at_time(&self, time: u64) -> Option<LinearMotion> {
        if time < self.start_time || time >= self.end_time {
            return None;
        }

        let mut motion_found = false;
        for entry in &self.linear_motions {
            let end_time = entry.start_time + ((entry.motion.duration * 16_000_000.0).ceil() as u64);
            let good = time >= entry.start_time && time < end_time;

            if !good {
                continue;
            }

            let rel_time = ((time - entry.start_time) as f64) / 16_000_000.0;
            return Some(entry.motion.clone().split_at(rel_time).1);
        }
 
        // None of the motions matched
        panic!()
    }

    pub fn position_at_time(&self, time: u64) -> Option<VectorXd> {
        self.position_derivatives_at_time(time).map(|m| m.start_position)
    }
}