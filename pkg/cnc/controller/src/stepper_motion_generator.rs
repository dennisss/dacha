use std::collections::VecDeque;
use std::sync::Arc;

use common::errors::*;
use cnc_controller_proto::cnc::*;
use cnc::linear_motion::*;
use cnc::quadratic_stepper_motion::*;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorMotion_Direction};

use crate::time::DeviceTime;
use crate::motion_utils::to_motor_space;

const MCU_CLOCK_FREQUENCY: usize = 16_000_000;

const TARGET_MIN_STEP_DUR: u32 = 400;

/*
Prusa XL : 400mm/s => 32000 steps/s

=> so need to support steps that are 500 clock cycles long.


*/

pub struct StepperMotionGenerator {
    config: Arc<MotionControllerConfig>,

    /// In the remote MCU clock space, this is '0' zero of all the stuff in this struct.
    /// (this is one entry per motor in case motors are on different MCUs)
    remote_start_time: Vec<DeviceTime>,

    /// Sequence of contiguous motions which we need to encode as steps.
    motions: VecDeque<LinearMotion>,

    /// Start time of the first motion in 'motions' in seconds.
    ///
    /// Internally all step times are computed relative to this value to keep the u32's
    /// close to zero for interpolation. This means that this must be periodically advanced
    /// internally to keep things going well.
    ///
    /// NOTE: This should always be <= motor_position_end_time[..]
    first_motion_start_time: f64,

    /// Current position of the motor.
    motor_position: Vec<i32>,

    // This is the time at which each motor first reached the motor_position.
    motor_position_reach_time: Vec<f64>,

    // This is basically the last time at which the each motor is still at its current position.
    //
    // Will be >= motor_position_reach_time.
    //
    // NOTE: These start at the same as first_motion_start_time but will gradually get larger
    // and may be in the middle of a motion.
    //
    // TODO: Need to keep this low so that it doesn't overflow when converted to a remote tick.
    motor_position_end_time: Vec<f64>,
}

impl StepperMotionGenerator {

    pub fn new(
        config: Arc<MotionControllerConfig>,
        motor_position: &[i32],
        remote_start_time: &[DeviceTime]
    ) -> Self {
        let num_motors = config.motors().len();

        assert_eq!(motor_position.len(), num_motors);
        assert_eq!(remote_start_time.len(), num_motors);

        Self {
            config,
            remote_start_time: remote_start_time.to_vec(),
            motions: VecDeque::new(),
            first_motion_start_time: 0.0,
            motor_position: motor_position.to_vec(),
            motor_position_reach_time: vec![0.0; num_motors],
            motor_position_end_time: vec![0.0; num_motors]
        }
    }

    pub fn motor_positions(&self) -> &[i32] {
        &self.motor_position
    }

    pub fn is_empty(&self) -> bool {
        self.motions.is_empty()
    }

    pub fn enqueue(&mut self, motion: LinearMotion) {
        self.motions.push_back(motion);        
    }

    
    // TODO: Need to periodically adjust the device time to account for clock skew and drift

    // TODO: Guarantee never sending times earlier than previously sent.

    // TODO: Once we try to get commands beyond the end of the queue time, we should prevent more motions from before adding without a full reset,

    /// Gets all commands 
    pub fn to_commands(
        &mut self, max_time: f64
    ) -> Result<Vec<Vec<StepperMotorMotion>>> {

        let num_motors = self.config.motors().len();

        let mut out = vec![vec![]; num_motors];

        // Starting time of the current motion we are looking at.
        let mut motion_start_time = self.first_motion_start_time;

        let mut first_used_motion = None;

        for (motion_i, motion) in self.motions.iter().enumerate() {
            if motion_start_time > max_time as f64 {
                break;
            }

            let motion_end_time = motion_start_time + (motion.duration as f64);

            // TODO: Have them preconverted.
            let motion_start_motor_position = to_motor_space(&motion.start_position, &self.config);
            let motion_end_motor_position = to_motor_space(&motion.end_position, &self.config);
            let motion_start_motor_velocity = to_motor_space(&motion.start_velocity, &self.config);
            let motion_motor_acceleration = to_motor_space(&motion.acceleration, &self.config);

            for motor_i in 0..num_motors {
                // Skip if the motor has already fully completed this motion.
                if self.motor_position_end_time[motor_i] >= motion_end_time - 0.0001 {
                    continue;
                }

                // TODO: Sanity check start position is ok.

                let motion_delta = motion_end_motor_position[motor_i] - motion_start_motor_position[motor_i];

                // Skip 
                if motion_delta.abs() < 0.01 {
                    // if motor_i == 0 {
                    //     println!("TO END: {}", motion_end_time);
                    //     println!("  {}, {}", motion_end_motor_position[motor_i], motion_start_motor_position[motor_i]);
                    //     println!("  Dur: {}", motion.duration);
                    // }


                    // Motor does not move for this motion.
                    self.motor_position_end_time[motor_i] = motion_end_time;
                    continue;
                }

                if first_used_motion.is_none() {
                    first_used_motion = Some(motion_i);
                }

                // Otherwise the motor is moving.
                let sign = if motion_delta > 0.0 { 1 } else { -1 };

                // This is what we define as time zero in 'ticks time' during all the calculations.
                // This will be added back to all the tick time values before returning to the caller.   
                let motion_offset = self.first_motion_start_time;

                // TODO: Verify the initial motor position is fairly close to the position demanded in the motion.

                let mut step_times = vec![
                    self.seconds_to_ticks(self.motor_position_end_time[motor_i] - motion_offset)
                ];
                loop {
                    let next_step_position = self.motor_position[motor_i] + sign;

                    let delta = (next_step_position as f32) - motion_start_motor_position[motor_i];

                    let mut raw_time = None;

                    // TODO: Check this.
                    // There is still some risk that this motion actually backtracks onto the current position
                    // so we need to skew the timing a bit.
                    let time = {
                        // TODO: Check all the signs of this stuff.
                        // TODO: The extrusion axis is still a bit lossy. The input linear motions don't perfectly start/end where they need to.
                        if delta.abs() < 0.01 {
                            0.0
                        } else if (delta - motion_delta).abs() < 0.01 {
                            motion.duration as f64
                        } else if delta.abs() > motion_delta.abs() {
                            break;
                        } else {
                            let time = cnc::displacement::time_to_travel(
                                delta,
                                motion_start_motor_velocity[motor_i],
                                motion_motor_acceleration[motor_i]
                            ) as f64;

                            raw_time = Some(time);

                            if time.is_nan() {
                                // TODO: This currently happens for cases like "-0.3203125 vs 4.0625"
                                // where the first step is reached before the start.
                                eprintln!("NaN step time: {:?} vs {:?}", delta, motion_delta);
                                break;
                            }

                            time.min(motion.duration as f64).max(0.0)
                        }
                    };

                    let step_end_time = motion_start_time + time;
                    if step_end_time > max_time {
                        break;
                    }

                    let step_end_ticks = self.seconds_to_ticks(step_end_time - motion_offset);

                    // The first step is often really short as we might have been moving very slowly over
                    // several motions and all of a sudden we finally got to the right position.
                    //
                    // TODO: Ideally this would try to look ahead one step and try to match the next steps
                    // velocity if we have time to do that.
                    //
                    // NOTE: This trick only works if first_motion_start_time is still behind the motor time by a
                    // little bit.
                    if step_times.len() == 1 {
                        // TODO: Error out if we have an overflowing subtract here.
                        let first_step_dur = step_end_ticks - step_times[0];
                        if first_step_dur < TARGET_MIN_STEP_DUR {
                            let max_shift = step_times[0].min(
                                self.seconds_to_ticks(
                                    (self.motor_position_end_time[motor_i] - self.motor_position_reach_time[motor_i])
                                        .min(1.0)
                                )
                            );

                            let want_shift = 2 * TARGET_MIN_STEP_DUR - first_step_dur;

                            // println!("Fixing {} by {}", first_step_dur, want_shift.min(max_shift));

                            step_times[0] -= want_shift.min(max_shift);
                        }
                    }

                    self.motor_position[motor_i] = next_step_position;
                    self.motor_position_reach_time[motor_i] = step_end_time;
                    self.motor_position_end_time[motor_i] = step_end_time;

                    step_times.push(step_end_ticks);
                }


                // TODO: Perform this across all motions we are doing for the best compression.
                self.step_times_to_commands(motor_i, sign > 0, &step_times, &mut out[motor_i])?;
            }

            motion_start_time = motion_end_time;
        }

        // NOTE: The else case of this should be handled by the next if statement
        // TODO: Maybe base everything in whether the motions are used?
        if let Some(i) = first_used_motion {
            for _ in 0..i {
                self.first_motion_start_time += self.motions[0].duration as f64;
                self.motions.pop_front();
            }
        }

        // TODO: Double check this against the above stuff and ensure that this is definitely
        // sufficient to ensure all steps have been emiited.
        //
        // TODO: Consider instead giving back the caller a more precise number in terms of how much is consumed
        // so that they can act accordingly.
        if max_time > motion_start_time {
            self.motions.clear();
            self.first_motion_start_time = max_time;
            for t in &mut self.motor_position_end_time {
                *t = max_time;
            }
        }

        Ok(out)
    }

    fn seconds_to_ticks(&self, v: f64) -> u32 {
        (v * MCU_CLOCK_FREQUENCY as f64).round() as u64 as u32
    }

    fn step_times_to_commands(
        &self,
        motor_i: usize,
        mut dir: bool,
        step_times: &[u32],
        out: &mut Vec<StepperMotorMotion>
    ) -> Result<()> {
        if step_times.len() == 1 {
            return Ok(());
        }

        // TODO: Also need to check against the last step before all of these.
        for i in 1..step_times.len() {            
            if step_times[i] <= step_times[i - 1] {
                return Err(err_msg("Non-monotonic step times"));
            }

            let step_duration = step_times[i] - step_times[i - 1];

            if step_duration < 400 {
                eprintln!("Short step!! {} ending at index {}", step_times[i] - step_times[i - 1], i);
            }

            if step_duration > 16_000_000 {
                eprintln!("Very long step: {}", step_duration);
            }

            // TODO: Need to check min and max time between steps.
        }


        // NOTE: These are allowed to be negative if that helps with alignment
        let mut raw_motions = vec![];

        // TODO: Validate that after compression, we still have the same number of steps.
        QuadraticStepperMotion::interpolate_step_times(&step_times, &mut raw_motions);

        let time_offset = self.remote_start_time[motor_i].lower()
            .wrapping_add(self.seconds_to_ticks(self.first_motion_start_time));
        for raw_motion in &mut raw_motions {
            raw_motion.next_step_time = raw_motion.next_step_time.wrapping_add(time_offset);
        }

        let mut skip_first = false;

        let mut last_time = {
            // if let Some(t) = self.motor_position_last_step_time[motor_i] {
            //     t
            // } else {
                skip_first = true;
                raw_motions[0].next_step_time
            // }
        };

        for raw_motion in &raw_motions {
            let mut m = raw_motion.clone();
    
            if skip_first {
                m.next();
                skip_first = false;
            }

            while m.num_steps.count() > 0 {
                let next_time = m.next_step_time;

                let delta_time = cnc::time_remaining_u32(next_time, last_time);

                // TODO: It is hard to have this compare to prior steps since we don't know if the motor was intentionally
                // idle for a while
                if delta_time > 16_000_000 {
                    println!("Raw Motion: {:?}", raw_motion);

                    return Err(format_err!("Really far out step: {} vs {}", next_time, last_time));
                }

                if delta_time < 40 {
                    return Err(format_err!("Really small step: {}", delta_time));
                }

                last_time = next_time;

                m.next();

            }
        }


        if self.config.motors()[motor_i].inverted() {
            dir = !dir;
        }


        for raw_motion in raw_motions {
            let mut step = StepperMotorMotion::default();

            // NOTE: Direction compression should be performed by the caller across motions right
            // before the motions are sent to the device.
            let dir_proto = match dir {
                true => StepperMotorMotion_Direction::FORWARD,
                false => StepperMotorMotion_Direction::BACKWARD
            };

            step.set_direction(dir_proto);

            // TODO: Compress this to be a delta relative to the last time.
            step.set_next_step_time(raw_motion.next_step_time);
            step.set_next_step_duration(if raw_motion.num_steps.count() == 1 { 0 } else { raw_motion.next_step_duration });
            step.set_step_duration_increment(raw_motion.step_duration_increment);
            step.set_num_steps_minus_one(raw_motion.num_steps.count() - 1);
            out.push(step);
        }

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    use math::vecxf;

    /*
    let start_velocity = vecxf!(0.0, 0.0, 0.0);

    let c = LinearMotionConstraints {
        start_position: vecxf!(0.0, 0.0, 0.0),
        end_position: vecxf!(50.0, 0.0, 0.0),
        max_end_speed: 0.0,
        max_speed: 100.0,
        max_acceleration: 100.0,
    };

    let mut t = 0.0;
    let mut last_pos = 0.0;

    let num_steps = 40;

    let mut csv = "time,x,dx\n".to_string();



    for i in 0..10 {
        let velocity = ((i + 1) as f32) * 10.0;


        let next_pos = last_pos + displacement_traveled(velocity, 0.0, 1.0);

        let motion = LinearMotion {
            start_position: vecxf!(last_pos, 0., 0.),
            start_velocity: vecxf!(velocity, 0., 0.),
            end_position: vecxf!(next_pos, 0., 0.),
            end_velocity: vecxf!(velocity, 0., 0.),
            acceleration: vecxf!(0., 0., 0.),
            duration: 1.0,
        };

        last_pos = next_pos;

        for i in 0..(num_steps + 1) {

            let ti = ((i as f32) / (num_steps as f32)) * motion.duration;
            let v = motion.clone().split_at(ti).1;

            csv.push_str(&format!("{:},{},{}\n", t + ti, v.start_position[0], v.start_velocity[0]));
        }

        println!("Time: {}", motion.duration);
        t += motion.duration;
    }

    */

    use cnc::displacement::time_to_travel;

    #[test]
    fn dump_curve() {
        let motion = LinearMotion {
            start_velocity: vecxf!(0.0, 0.0, 0.0),
            end_position: vecxf!(400.0, 0.0, 0.0), // 
            acceleration: vecxf!(80.0 * 1000.0, 0.0, 0.0), //
            duration: 0.1,

            // Not important.
            start_position: vecxf!(0.0, 0.0, 0.0),
            end_velocity: vecxf!(0.0, 0.0, 0.0),
        };

        let num_steps = 400;

        let mut csv = "time,x,dx\n".to_string();

        for i in 0..(num_steps + 1) {

            let ti = ((i as f32) / (num_steps as f32)) * motion.duration;
            let v = motion.clone().split_at(ti).1;

            csv.push_str(&format!("{:},{},{}\n", ti, v.start_position[0], v.start_velocity[0]));
        }

        println!("{}", csv);

        println!("=====");

        let mut csv = "time,x\n".to_string();

        for i in 0..401 {
            let time = time_to_travel(i as f32, 0.0, 80.0 * 1000.0);
            println!("{},{}", time, i);
        }

    }

    #[test]
    fn works() {
        let mut config = MotionControllerConfig::default();
        protobuf::text::parse_text_proto(r#"
            motors: [
                { steps_per_mm: 1 }
            ]
            geometry: [
                { direct { axis_index: 0 motor_index: 0 } }
            ]
        "#, &mut config).unwrap();

        let config = Arc::new(config);

        let run_motion = |motion: LinearMotion| {

            println!("====");

            let mut motor_positions = vec![0];
            let mut time = vec![DeviceTime::new_test_only(0)];

            let mut generator = StepperMotionGenerator::new(config.clone(), &motor_positions, &time);
            generator.enqueue(motion.clone());

            let step_motions = generator.to_commands((motion.duration + 1.0) as f64).unwrap();

            // TODO: Update this.
            /*
            let step_motions = motion_to_step_commands(
                &motion,
                &mut motor_positions,
                &mut time,
                &mut last_motion_residuals,
                &config
            ).unwrap();


            */

            println!("{:?}", step_motions);
            // println!("{:?}", motor_positions);
        };

        run_motion(LinearMotion {
            start_velocity: vecxf!(0.0, 0.0, 0.0),
            end_position: vecxf!(400.0, 0.0, 0.0), // 
            acceleration: vecxf!(80.0 * 1000.0, 0.0, 0.0), //
            duration: 0.1,

            // Not important.
            start_position: vecxf!(0.0, 0.0, 0.0),
            end_velocity: vecxf!(0.0, 0.0, 0.0),
        });

        return;

        // Constant velocity
        run_motion(LinearMotion {
            start_velocity: vecxf!(1.0, 0.0, 0.0),
            end_position: vecxf!(1.0, 0.0, 0.0), // 
            acceleration: vecxf!(0.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vecxf!(0.0, 0.0, 0.0),
            end_velocity: vecxf!(0.0, 0.0, 0.0),
        });

        // Doing nothing
        run_motion(LinearMotion {
            start_velocity: vecxf!(0.0, 0.0, 0.0),
            end_position: vecxf!(0.0, 0.0, 0.0), // 
            acceleration: vecxf!(0.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vecxf!(0.0, 0.0, 0.0),
            end_velocity: vecxf!(0.0, 0.0, 0.0),
        });

        // Accelerate from zero velocity.
        run_motion(LinearMotion {
            start_velocity: vecxf!(0.0, 0.0, 0.0),
            end_position: vecxf!(5.0, 0.0, 0.0), // 
            acceleration: vecxf!(10.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vecxf!(0.0, 0.0, 0.0),
            end_velocity: vecxf!(0.0, 0.0, 0.0),
        });




    }


}