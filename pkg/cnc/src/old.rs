pub struct LinearTrajectory {
    start_position: Vector3f,
    start_velocity: Vector3f,

    end_position: Vector3f,
    end_velocity: Vector3f,

    acceleration: Vector3f,

    duration: f32,
}

/*
Every unplanned motion can be characterized by two end velocity constraints:
1. Max cornering velocity to enter the next segment
2. Max velocity such that we can safely bring the machine to the stop.
    ^ This velocity changes

A motion can be 'planned' onced min(#1, #2) no longer changes (NOTE: it will only ever get higher with more motions queued).
- We can test this by adding a fake 'stop' motion to the end of the queue and back progating the final end_velocity to the final motion's start_velocity, ...



We have a simple solution if we start and stop at the same velocity.
-




Knowns:
- At the end, we want to be at 0 velocity
-


Will need to do back propagation through time:
-



What are we minimizing? total time






mid_velocity = start_velocity + acceleration * time_up






*/

impl LinearTrajectory {
    pub fn create(
        start_position: Vector3f,
        start_velocity: Vector3f,
        end_position: Vector3f,
        max_velocity: Vector3f,
        max_end_velocity: Vector3f,
        max_acceleration: Vector3f,
        plan: &mut Vec<LinearTrajectory>,
    ) {
        let distance_vector = &end_position - &start_position;
        if distance_vector.norm() <= 1e-6 {
            return;
        }

        let direction_vector = distance_vector.normalized();

        // If we are traveling in a different direction initially, assume we can
        // instantly stop.
        let mut start_velocity_component = direction_vector.dot(&start_velocity).abs();
        if start_velocity_component < 0.0 {
            start_velocity_component = 0.0;
        }

        // Variables
        // - Amount of time spent speeding up (or down).
        // - Amount of time to ramp down

        /*
        Simple case:
        - Can ramp up to max velocity
        - Can cruise at max velocity
        - Can slow down to exactly max_end_velocity


        */

        /*

        I have a few constraints:
        1. max velocity
        2.


        x1 = t1 *


        */

        // Step 1. Check how much distance is required

        // We have two lines:
        // 1.
    }
}

/*
Know:
- Initial Position Vector (Global coordinates)
- Initial Velocity Vector (Global coordinates)
- Final Position Vector
- Max Velocity
- Max Final Velocity
- Max Acceleration

What I need to know:
- A set of motions where each has:
    Initial Position Vector
    Initial Velocity Vector (truncating tangential velocity)
    Acceleration
    Final Position Vector
    Final Velocity Vector
    Duration



acAA

4

What is a linear motion:
- Has a start position (x,y,z,e)
- Has an end position (x,y,z,e)


To build a linear motion, we need to know:
- Start velocity (projected onto path)
- End velocity (projected onto path)
-

Each linear motion will be formed as:
- Given starting velocity projected onto the path.
- Speed up to


Need to optimize multiple dimensions at once:
- Just doing one dimension will at a time will mess up the position.

- We can assume max velocity

*/

/*
Line interpolation:
- Parameters
    - Start position
    - Start velocity
    - Mid Point
    - At a diven

The path planner should contain enough forward information


Step 1 is to

buffer representation:
-
- Internal commands of the form:
    - Linear(num_steps_x, num_steps_y, start_time, end_time)
        - Constant pulse width.
    - Need step width as a function of time

For now, we will only operate with single axis motion planners and

Each segment has a start_time, end_time, and num_ticks

interpolate the value of the current tick to be ((time - start_time) / (end_time - start_time)) * num_ticks

To execute an acceleration, I need to know the duration of each step.

position = velocity * time
velocity = accel

*/

struct MotionPlan {
    start_time: u32,
    end_time: u32,
}

struct LinearMotionPlan {
    plan: Vec<LinearMotionOld>,
}

/// NOTE: An assumption we make is that a motion never switches direction (the
/// velocity never crosses zero but may start/stop at zero).
struct LinearMotionOld {
    initial_position: f32,
    initial_velocity: f32,

    /// Steps per second per second.
    acceleration: f32,

    duration: f32,
    // start_time: f32,
    max_acceleration: f32,
}

impl LinearMotionPlan {
    pub fn append(
        &mut self,
        mut initial_position: f32,
        mut initial_velocity: f32,
        final_position: f32,
        max_velocity: f32,
        max_acceleration: f32,
    ) {
        // If the last motion was faster than the current one, modify the previous
        // motions to ramp down the speed.
        if max_velocity < initial_velocity.abs() {
            // TODO: If previous motions were created with a different max_acceleration
            // target, support respecting that instead

            // Starting at the last motion, try to intersect it with the target velocity.

            // TODO: Need a special case for an empty plan.

            // NOTE: We assume that if the velocity is non-zero then the plan is long enough
            // to decelerate.
            let mut i = self.plan.len() - 1;

            while i >= 0 {
                let mut last_motion = &mut self.plan[i];

                let current_max_acceleration = last_motion.max_acceleration;
                // core::cmp::min(max_acceleration, last_motion.max_acceleration);

                // Amount of time it would take to slow down.
                let rampdown_time = (initial_velocity - max_velocity) / current_max_acceleration;

                let rampdown_distance =
                    (current_max_acceleration / 2.0) * (rampdown_time * rampdown_time);

                if rampdown_distance >= (initial_position - last_motion.initial_position).abs() {
                    // The last motion is not long enough to slow down.
                    // Set the last motion to the lowest initial velocity we can and then continue

                    let new_last_motion_time = Self::time_to_reach(
                        initial_position,
                        initial_velocity,
                        last_motion.initial_position,
                        current_max_acceleration,
                    );

                    let new_last_motion_initial_velocity =
                        initial_velocity - new_last_motion_time * current_max_acceleration;

                    last_motion.initial_velocity = new_last_motion_initial_velocity;
                    // last_motion.acceleration = // TODO:
                    last_motion.duration = new_last_motion_time;

                    i -= 1;
                    continue;
                }

                // Otherwise, we can split the last motion into 2:
                // - First a regular motion at the existing acceleration
                // - Second a ramp down

                /*
                if last_motion.initial_velocity.abs() <= max_velocity {
                    // If the previous motion started slower than the current motion, we can just
                    // stop it's acceleration early.

                    let midpoint_duration =
                        (max_velocity - last_motion.initial_velocity.abs()) / max_acceleration;

                    // let midpoint_position =

                    last_motion.duration = midpoint_duration;

                    last_motion.initial_velocity = max_velocity;

                    break;
                } else {
                    // Otherwise, change the motion so that it ends in max_velocity

                    i -= 1;
                }
                */
            }

            // let last_motion =

            // let time_to_slow_down =
        }

        // If a previous motion was running at a faster speed, we assume that we have
        // slowed down before starting the next motion.
        assert!(max_velocity >= initial_velocity.abs());

        // TODO: This doesn't work if the initial velocity is zero.
        let direction = (final_position - initial_position).signum() as f32;

        let min_duration = Self::time_to_reach(
            initial_position,
            initial_velocity,
            final_position,
            max_acceleration,
        );
        let min_duration_velocity = min_duration * max_acceleration * direction + initial_velocity;

        // We won't reach the max duration before the end of the motion, so we can do it
        // in one
        if min_duration_velocity.abs() <= max_velocity {
            self.plan.push(LinearMotionOld {
                initial_position,
                initial_velocity,
                acceleration: max_acceleration * direction,
                duration: min_duration,
                max_acceleration,
            });
            return;
        }

        // Otherwise we need to first accelerate to the max velocity and then travel at
        // a constant velocity for the remaining time.

        let rampup_time = (max_velocity - initial_velocity.abs()) / max_acceleration;
        let rampup_end_position = (direction * max_acceleration / 2.0)
            * (rampup_time * rampup_time)
            + (initial_velocity * rampup_time)
            + initial_position;
        self.plan.push(LinearMotionOld {
            initial_position,
            initial_velocity,
            acceleration: max_acceleration * direction,
            duration: rampup_time,
            max_acceleration,
        });

        initial_position = rampup_end_position;
        initial_velocity = max_velocity * direction;
        self.plan.push(LinearMotionOld {
            initial_position,
            initial_velocity,
            acceleration: 0.0,
            duration: (final_position - initial_position) / initial_velocity,
            max_acceleration,
        });
    }

    /*
    ///
    pub fn step_start_time(&self, i: usize) -> f32 {
        let a = (self.acceleration / 2.0f32);
        let b = self.initial_velocity;
        let c = (self.initial_position as f32) - ((self.num_steps.signum() as f32) * (i as f32));

        let x = (-b + (b * b - 4.0f32 * a * c).sqrt()) / (2.0f32 * a);

        x
    }
    */

    fn time_to_reach(
        initial_position: f32,
        initial_velocity: f32,
        final_position: f32,
        acceleration: f32,
    ) -> f32 {
        // No velocity zero crossings.
        assert!((final_position > initial_position) == (acceleration > 0.0));

        let a = acceleration / 2.0;
        let b = initial_velocity;
        let c = initial_position - final_position;

        // Quadratic equation
        (-b + (b * b - 4.0 * a * c).sqrt()) / (2.0 * a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {
        // let motion = LinearMotion {
        //     initial_position: 0.0f,
        //     initial_velocity: 0.0f,
        //     acceleration: 1.0f,
        //     start_time: 0,

        // };
    }
}
