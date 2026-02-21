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

/// This is the smallest granularity of time we will treat as containing meaningful motion
/// (>= 1 step of progress). This is used as the f64 epsilon for comparing times.
///
/// Should be much smaller than the shortest step time expected but larger enough to avoid
/// f64 rounding errors.
const MIN_STEP_SECONDS: f64 = 0.00001;

/// Minimum duration of the first step in each discrete motion in tick units.
///
/// (this is for discontinuity compensation between motions. read the code that uses this)
const FIRST_STEP_MIN_DURATION_TICKS: u32 = 250;

/// Maximum amount of time into the past that we will increase the duration of the first
/// step in a discrete motion.
///
/// If we generate steps 100ms into the future, then this value must be << that to ensure
/// we don't send the MCU a time that has already elapsed.
///
/// (this is for discontinuity compensation between motions. read the code that uses this)
const FIRST_STEP_MAX_SHIFT_SECONDS: f64 = 0.001; // 1ms

/*
Prusa XL : 400mm/s => 32000 steps/s

=> so need to support steps that are 500 clock cycles long.


// TODO: Eventually also implement stepper motor level acceleration/velocity limits.


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
///
    /// NOTE: Should always be called with a monotonically increasing max_time value.
    pub fn to_commands(
        &mut self, max_time: f64
    ) -> Result<Vec<Vec<(DeviceTime, QuadraticStepperMotion)>>> {

        let num_motors = self.config.motors().len();

        let mut out = vec![vec![]; num_motors];

        // Starting time of the current motion we are looking at.
        let mut motion_start_time = self.first_motion_start_time;

        let mut first_used_motion = None;

        for (motion_i, motion) in self.motions.iter().enumerate() {
            if motion_start_time > max_time {
                break;
            }

            let motion_end_time = motion_start_time + motion.duration;

            // TODO: Have them preconverted.
            let motion_start_motor_position = to_motor_space(&motion.start_position, &self.config);
            let motion_end_motor_position = to_motor_space(&motion.end_position, &self.config);
            let motion_start_motor_velocity = to_motor_space(&motion.start_velocity, &self.config);
            let motion_motor_acceleration = to_motor_space(&motion.acceleration, &self.config);

            for motor_i in 0..num_motors {
                // Skip if the motor has already fully completed this motion.
                if self.motor_position_end_time[motor_i] >= motion_end_time - MIN_STEP_SECONDS {
                    continue;
                }

                // TODO: Sanity check start position is ok.

                let motion_delta = motion_end_motor_position[motor_i] - motion_start_motor_position[motor_i];

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

                    let delta = (next_step_position as f64) - motion_start_motor_position[motor_i];

                    let end_delta = (next_step_position as f64) - motion_end_motor_position[motor_i];

                    let mut raw_time = None;

                    // TODO: Check this.
                    // There is still some risk that this motion actually backtracks onto the current position
                    // so we need to skew the timing a bit.
                    let time = {
                        // TODO: Check all the signs of this stuff.
                        // TODO: The extrusion axis is still a bit lossy. The input linear motions don't perfectly start/end where they need to.
                        if end_delta.abs() < 0.01 {
                                                        motion.duration
                        } else if delta.abs() > motion_delta.abs() {
                            break;
                        } else {
                            let time = cnc::displacement::time_to_travel(
                                delta,
                                motion_start_motor_velocity[motor_i],
                                motion_motor_acceleration[motor_i]
                            );

                            raw_time = Some(time);

                            if time.is_nan() {
                                // This generally means the start or end begin or end at zero velocity and we have negative acceleration compared to the movement direction. Ideally the '< 0.01' checks above catch this.

                                return Err(format_err!("NaN step time: {:?} vs {:?}", delta, motion_delta));
                            }

                            time.min(motion.duration).max(0.0)
                        }
                    };

                    let step_end_time = motion_start_time + time;
                    if step_end_time > max_time + MIN_STEP_SECONDS {
                        break;
                    }

                    let step_end_ticks = self.seconds_to_ticks(step_end_time - motion_offset);

                    // The first step is often really short as we might have been moving very slowly over
                    // several motions and all of a sudden we finally got to the right position.
//
                    // This is generally signalled by there being a large gap between motor_position_end_time
                    // and motor_position_reach_time implying there is some uncertainty about whether we are
                    // moving very slowly or we just started moving
                    //
                    // TODO: Ideally this would try to look ahead one step and try to match the next steps
                    // velocity if we have time to do that.
                    //
                    // NOTE: This trick only works if first_motion_start_time is still behind the motor time by a
                    // little bit.
                    if step_times.len() == 1 {
// We estimate that that first step should no shorter than 95% of the start velocity.
                        let first_step_min_dur = (1.0 / motion_start_motor_velocity[motor_i].abs()) * 0.95;

                        // The target minimum size for the first step.
                        // (clamping instantaneous velocity estimate to reasonable hard limits)
                        let first_step_dur_target = self.seconds_to_ticks(first_step_min_dur)
                            .max(FIRST_STEP_MIN_DURATION_TICKS)
                            .min(self.seconds_to_ticks(FIRST_STEP_MAX_SHIFT_SECONDS));

                        // TODO: Error out if we have an overflowing subtract here.
                        let first_step_dur = step_end_ticks - step_times[0];
                        if first_step_dur < first_step_dur_target {
                            let max_shift = step_times[0].min(
                                self.seconds_to_ticks(
                                    (self.motor_position_end_time[motor_i] - self.motor_position_reach_time[motor_i])
                                        .min(1.0)
                                )
                            );

                            let want_shift = first_step_dur_target - first_step_dur;
                            step_times[0] -= want_shift.min(max_shift);
                        }
                    }

                    self.motor_position[motor_i] = next_step_position;
                    self.motor_position_reach_time[motor_i] = step_end_time;
                    self.motor_position_end_time[motor_i] = step_end_time;

                    step_times.push(step_end_ticks);
                }

                // Even if we produced no steps in this time window, we still need to advance the
                // end time for the motor so that we don't end up issuing steps that are far into the past
                // in future iterations.
                //
                // This mainly comes up in two scenarios:
                // 1. If a motion doesn't move a specific motor, we need to skip past it in time.
                // 2. If max_time only captures a relatively small slice of the current motion, we may
                //    not have observed a full step yet.
                //    - This case is tricky as a very slow velocity may mean that 1 step takes several
                //      seconds
                //    - TODO: Allow this time to be slightly in the past as long as it doesn't risk our
                //      scheduling buffer for steps. 
                if step_times.len() == 1 {
                    // Alternative is to mark the end as motion_end_time.min(max_time);

                    // Motor did not move for this motion.
                    self.motor_position_end_time[motor_i] = motion_end_time.min(max_time);
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
        out: &mut Vec<(DeviceTime, QuadraticStepperMotion)>
    ) -> Result<()> {
        if step_times.len() == 1 {
            return Ok(());
        }

        // TODO: Also need to check against the last step before all of these.
        for i in 1..step_times.len() {            
            if step_times[i] <= step_times[i - 1] {
                return Err(format_err!("Non-monotonic step times : step_times[{}] = {}; step_times[{}] = {}",
                    i, step_times[i], i - 1, step_times[i - 1]));
            }

            let step_duration = step_times[i] - step_times[i - 1];

            if step_duration < 400 {
                eprintln!("Short step!! (motor {}) {} ending at index {}", motor_i, step_times[i] - step_times[i - 1], i);
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

        let time_offset = self.remote_start_time[motor_i]
            .add_secs(self.first_motion_start_time);

        for raw_motion in &mut raw_motions {
            let next_step_time = time_offset.add_ticks(raw_motion.next_step_time);

raw_motion.next_step_time = next_step_time.lower();
            raw_motion.num_steps.set_direction(dir);

            out.push((next_step_time, raw_motion.clone()));
        }

        let mut first = true;

        let mut last_time = raw_motions[0].next_step_time;


        for raw_motion in &raw_motions {
            let mut m = raw_motion.clone();
    
            if first {
                m.next();
                first = false;
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

                if delta_time < 200 {
                    return Err(format_err!("Really small step: {}", delta_time));
                }

                last_time = next_time;

                m.next();

            }
        }

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    use math::vecxd;

    /*
    let start_velocity = vecxd!(0.0, 0.0, 0.0);

    let c = LinearMotionConstraints {
        start_position: vecxd!(0.0, 0.0, 0.0),
        end_position: vecxd!(50.0, 0.0, 0.0),
        max_end_speed: 0.0,
        max_speed: 100.0,
        max_acceleration: 100.0,
    };

    let mut t = 0.0;
    let mut last_pos = 0.0;

    let num_steps = 40;

    let mut csv = "time,x,dx\n".to_string();



    for i in 0..10 {
        let velocity = ((i + 1) as f64) * 10.0;


        let next_pos = last_pos + displacement_traveled(velocity, 0.0, 1.0);

        let motion = LinearMotion {
            start_position: vecxd!(last_pos, 0., 0.),
            start_velocity: vecxd!(velocity, 0., 0.),
            end_position: vecxd!(next_pos, 0., 0.),
            end_velocity: vecxd!(velocity, 0., 0.),
            acceleration: vecxd!(0., 0., 0.),
            duration: 1.0,
        };

        last_pos = next_pos;

        for i in 0..(num_steps + 1) {

            let ti = ((i as f64) / (num_steps as f64)) * motion.duration;
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
            start_velocity: vecxd!(0.0, 0.0, 0.0),
            end_position: vecxd!(400.0, 0.0, 0.0), // 
            acceleration: vecxd!(80.0 * 1000.0, 0.0, 0.0), //
            duration: 0.1,

            // Not important.
            start_position: vecxd!(0.0, 0.0, 0.0),
            end_velocity: vecxd!(0.0, 0.0, 0.0),
        };

        let num_steps = 400;

        let mut csv = "time,x,dx\n".to_string();

        for i in 0..(num_steps + 1) {

            let ti = ((i as f64) / (num_steps as f64)) * motion.duration;
            let v = motion.clone().split_at(ti).1;

            csv.push_str(&format!("{:},{},{}\n", ti, v.start_position[0], v.start_velocity[0]));
        }

        println!("{}", csv);

        println!("=====");

        let mut csv = "time,x\n".to_string();

        for i in 0..401 {
            let time = time_to_travel(i as f64, 0.0, 80.0 * 1000.0);
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
            start_velocity: vecxd!(0.0, 0.0, 0.0),
            end_position: vecxd!(400.0, 0.0, 0.0), // 
            acceleration: vecxd!(80.0 * 1000.0, 0.0, 0.0), //
            duration: 0.1,

            // Not important.
            start_position: vecxd!(0.0, 0.0, 0.0),
            end_velocity: vecxd!(0.0, 0.0, 0.0),
        });

        return;

        // Constant velocity
        run_motion(LinearMotion {
            start_velocity: vecxd!(1.0, 0.0, 0.0),
            end_position: vecxd!(1.0, 0.0, 0.0), // 
            acceleration: vecxd!(0.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vecxd!(0.0, 0.0, 0.0),
            end_velocity: vecxd!(0.0, 0.0, 0.0),
        });

        // Doing nothing
        run_motion(LinearMotion {
            start_velocity: vecxd!(0.0, 0.0, 0.0),
            end_position: vecxd!(0.0, 0.0, 0.0), // 
            acceleration: vecxd!(0.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vecxd!(0.0, 0.0, 0.0),
            end_velocity: vecxd!(0.0, 0.0, 0.0),
        });

        // Accelerate from zero velocity.
        run_motion(LinearMotion {
            start_velocity: vecxd!(0.0, 0.0, 0.0),
            end_position: vecxd!(5.0, 0.0, 0.0), // 
            acceleration: vecxd!(10.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vecxd!(0.0, 0.0, 0.0),
            end_velocity: vecxd!(0.0, 0.0, 0.0),
        });




    }


}