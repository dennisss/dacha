use common::errors::*;
use cnc_controller_proto::cnc::*;
use math::matrix::Vector3f;
use cnc::linear_motion::*;
use cnc::quadratic_stepper_motion::*;
use peripherals_proto::peripherals::{StepperMotorMotion, StepperMotorMotion_Direction};

use crate::time::DeviceTime;

const MCU_CLOCK_FREQUENCY: usize = 16_000_000;

/*
Prusa XL : 400mm/s => 32000 steps/s

=> so need to support steps that are 500 clock cycles long.


TODO: The last_motion_residuals are currently needed since we split up motions in the LinearMotionPlanner
so we may end up with getting a motion that immediately completes a step in near zero ticks while in reality
the step should have partially started in the previous motion but since the previous motion didn't complete the
step, we didn't mark it. Think about a better way to avoid this.

*/


/// start_motor_positions: Step positions of each motor at the start of the move.
///    Updated to contain the final motor positions after the move. 
///
/// start_time: Time to start the first step. Updated to contain
///             the first time after all steps are done.
pub fn motion_to_step_commands(
    motion: &LinearMotion,
    start_motor_position: &mut [i64],
    start_time: &mut DeviceTime,
    last_motion_residuals: &mut [u32],
    config: &MotionControllerConfig
) -> Result<Vec<Vec<StepperMotorMotion>>> {

    
    let motion_start_motor_position = to_motor_space(&motion.start_position, &config);
    let motion_end_motor_position = to_motor_space(&motion.end_position, &config);
    let motion_start_motor_velocity = to_motor_space(&motion.start_velocity, &config);
    let motion_motor_acceleration = to_motor_space(&motion.acceleration, &config);

    let mut out = vec![vec![]; config.motors().len()];

    let mut next_start_time = *start_time;

    let mut step_times = vec![];

    // TODO: This is currently very crude and non-constant acceleration for parts of the motion.
    for i in 0..config.motors().len() {
        step_times.clear();

        let full_motion_delta = motion_end_motor_position[i] - motion_start_motor_position[i];

        let sign =  {
            if full_motion_delta > 0.0 { 1 } else { -1 }
        };

        // TODO: I only care about the start times being correct (don't need the end point..

        // TODO: Think more about this.
        // There is likely still some residual time in the previous motion where we started to move to the next position but didn't fully reach it.
        step_times.push(0);
        
        let mut cur_step_position = start_motor_position[i];
        let mut cur_step_end_time = 0.0;
        loop {
            let next_step_position = cur_step_position + sign;

            // TODO: This is currently before the start position of the 
            let delta = (next_step_position as f32) - motion_start_motor_position[i];

            let time = {
                // TODO: Check all the signs of this stuff.
                if delta.abs() < 0.01 {
                    0.0
                } else if (delta - full_motion_delta).abs() < 0.01 {
                    motion.duration
                } else if delta.abs() > full_motion_delta.abs() {
                    break;
                } else {
                    let time = cnc::displacement::time_to_travel(
                        delta,
                        motion_start_motor_velocity[i],
                        motion_motor_acceleration[i]
                    );

                    if time.is_nan() {
                        // TODO: This currently happens for cases like "-0.3203125 vs 4.0625"
                        // where the first step is reached before the start.
                        eprintln!("NaN step time: {:?} vs {:?}", delta, full_motion_delta);
                        break;
                    }

                    time.min(motion.duration).max(0.0)
                }
            };

            cur_step_end_time = time;

            // TODO: Only use the residuals if we are going in the same direction as the previous motion?
            // (though if we don't use them there is more of a risk that we skip steps)
            step_times.push(last_motion_residuals[i] + (time * MCU_CLOCK_FREQUENCY as f32).round() as u32);
            cur_step_position = next_step_position;
        }

        
        last_motion_residuals[i] = ((motion.duration - cur_step_end_time) * (MCU_CLOCK_FREQUENCY as f32)).round() as u32;

        if ((cur_step_position as f32) - motion_end_motor_position[i]).abs() > 1.5 {
            return Err(err_msg("Did not reach the final motion position."));
        }

        // TODO: Explicitly check that the residual is relatively small.

        if step_times.len() == 1 {
            continue;
        }

        for i in 1..step_times.len() {
            assert!(step_times[i] > step_times[i - 1], "Bad steps: {:?}; {:?}; {:?} ; {:?}; {:?}", step_times, motion_start_motor_velocity, motion_motor_acceleration, motion, start_motor_position);

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
        QuadraticStepperMotion::interpolate_step_times(&step_times, &mut raw_motions);

        let mut dir = sign > 0;
        if config.motors()[i].inverted() {
            dir = !dir;
        }

        for raw_motion in raw_motions {
            let mut step = StepperMotorMotion::default();

            // NOTE: Direction compression should be performed by the caller across motions.
            let dir_proto = match dir {
                true => StepperMotorMotion_Direction::FORWARD,
                false => StepperMotorMotion_Direction::BACKWARD
            };

            step.set_direction(dir_proto);
            step.set_next_step_time(start_time.lower().wrapping_add(raw_motion.next_step_time));
            step.set_next_step_duration(raw_motion.next_step_duration);
            step.set_step_duration_increment(raw_motion.step_duration_increment);
            step.set_num_steps(raw_motion.num_steps.count());
            out[i].push(step);
        }

        // TODO: update next_start_time and start_positiosn
        start_motor_position[i] = cur_step_position;

        next_start_time = next_start_time.max(start_time.add_ticks(step_times[step_times.len() - 1]));
    }

    *start_time = next_start_time;

    Ok(out)
}




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


#[cfg(test)]
mod tests {
    use super::*;

    use math::matrix::vec3f;

    /*
    #[test]
    fn works2() {
        let mut i = 0;

        let mut time = 0;
        let mut dur = 100;
        let step = 10;

        for _ in 0..100 {
            println!("{},{}", time, i);
            i += 1;
            time += dur;
            dur += step;
        }



        // let a = 4294966916u32;
        // println!("{}", a as i32);

    }
    */

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

        let run_motion = |motion: LinearMotion| {

            println!("====");

            let mut motor_positions = vec![0];
            let mut last_motion_residuals = vec![0];
            let mut time = DeviceTime::new_test_only(0);

            let step_motions = motion_to_step_commands(
                &motion,
                &mut motor_positions,
                &mut time,
                &mut last_motion_residuals,
                &config
            ).unwrap();

            println!("{:?}", step_motions);
            println!("{:?}", motor_positions);
        };

        run_motion(LinearMotion {
            start_velocity: vec3f(0.0, 0.0, 0.0),
            end_position: vec3f(400.0, 0.0, 0.0), // 
            acceleration: vec3f(80.0 * 1000.0, 0.0, 0.0), //
            duration: 0.1,

            // Not important.
            start_position: vec3f(0.0, 0.0, 0.0),
            end_velocity: vec3f(0.0, 0.0, 0.0),
        });

        return;

        // Constant velocity
        run_motion(LinearMotion {
            start_velocity: vec3f(1.0, 0.0, 0.0),
            end_position: vec3f(1.0, 0.0, 0.0), // 
            acceleration: vec3f(0.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vec3f(0.0, 0.0, 0.0),
            end_velocity: vec3f(0.0, 0.0, 0.0),
        });

        // Doing nothing
        run_motion(LinearMotion {
            start_velocity: vec3f(0.0, 0.0, 0.0),
            end_position: vec3f(0.0, 0.0, 0.0), // 
            acceleration: vec3f(0.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vec3f(0.0, 0.0, 0.0),
            end_velocity: vec3f(0.0, 0.0, 0.0),
        });

        // Accelerate from zero velocity.
        run_motion(LinearMotion {
            start_velocity: vec3f(0.0, 0.0, 0.0),
            end_position: vec3f(5.0, 0.0, 0.0), // 
            acceleration: vec3f(10.0, 0.0, 0.0), //
            duration: 1.0,

            // Not important.
            start_position: vec3f(0.0, 0.0, 0.0),
            end_velocity: vec3f(0.0, 0.0, 0.0),
        });




    }


}
