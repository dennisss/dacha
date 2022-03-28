use alloc::{collections::VecDeque, vec::Vec};

use math::matrix::cwise_binary_ops::*;
use math::matrix::Vector3f;

use crate::kinematics::*;
use crate::linear_motion::*;
use crate::linear_motion_constraints::*;

pub struct LinearMotionPlanner {
    start_position: Vector3f,
    start_velocity: Vector3f,
    queue: VecDeque<LinearMotionConstraints>,
}

impl LinearMotionPlanner {
    pub fn new(start_position: Vector3f) -> Self {
        Self {
            start_position,
            start_velocity: Vector3f::zero(),
            queue: VecDeque::new(),
        }
    }

    // TODO: Add a dwell. This could fully constrain all previous routes.
    // - Will need to disallow modifying finalized paths.

    // pub fn

    // pub fn move_to(&mut self, end_position: Vector3f, )

    // TODO: max_speed should be equal to the feed rate with per-axis limits
    // applied.
    //
    // TODO: max_acceleration should be the magnitude of per-axis
    // max_acceleration components in the direction of the motion.
    //
    // TODO: If there are extremely long linear motions, split them into pieces so
    // that te planner can emit partial results quickly (similarly combine many
    // short movements in the same direction).
    pub fn move_to(&mut self, end_position: Vector3f, max_speed: f32, max_acceleration: f32) {
        let start_position = {
            if let Some(last_motion) = self.queue.back() {
                last_motion.end_position.clone()
            } else {
                self.start_position.clone()
            }
        };

        // TODO: Verify no discontinuity of positions.

        // If we had a previous motion, compute the max cornering speed.
        if let Some(last_motion) = self.queue.back_mut() {
            // See https://onehossshay.wordpress.com/2011/09/24/improving_grbl_cornering_algorithm/

            const MAX_DEVIATION: f32 = 0.01;

            // Motion directions relative to the corner between last and current motion.
            let entry_direction =
                (&last_motion.end_position - &last_motion.start_position).normalized();
            let exit_direction = (&end_position - &start_position).normalized();

            // TODO: Support separately computing Z cornering speed.
            let mut max_cornering_speed = Self::compute_max_cornering_speed(
                entry_direction,
                exit_direction,
                MAX_DEVIATION,
                max_acceleration,
            );

            last_motion.max_cornering_speed = max_cornering_speed
                .min(last_motion.max_speed)
                .min(max_speed);
        }

        // Append to queue.
        self.queue.push_back(LinearMotionConstraints {
            start_position,
            end_position,
            max_start_speed: 0.0,
            max_end_speed: 0.0,
            max_speed,
            max_cornering_speed: 0.0,
            max_acceleration,
            fully_constrained: false,
        });

        self.backpropagate_speed_limits();
    }

    // NOTE: entry_direction and exit_direction should be normalized.
    fn compute_max_cornering_speed(
        entry_direction: Vector3f,
        exit_direction: Vector3f,
        max_deviation: f32,
        max_acceleration: f32,
    ) -> f32 {
        let cornering_angle = entry_direction.dot(&exit_direction).acos();

        let corner_radius =
            max_deviation * ((cornering_angle / 2.0).sin() / (1.0 - (cornering_angle / 2.0).sin()));

        let cornering_speed = (max_acceleration * corner_radius).sqrt();

        cornering_speed
    }

    fn backpropagate_speed_limits(&mut self) {
        // The final motion must end at rest.
        let mut next_max_start_speed: f32 = 0.0;

        for i in (0..self.queue.len()).rev() {
            let motion = &mut self.queue[i];

            motion.max_end_speed = next_max_start_speed.max(motion.max_cornering_speed);

            // Amount of space in which we can accelerate/decelerate.
            let distance = (&motion.end_position - &motion.start_position).norm();

            // Assuming we accelerated at the max allowed rate, how long would it take to
            // speed up/down from/to the end velocity while not overshooting the distance of
            // the linear motion.
            let ramp_down_time =
                time_to_travel(distance, motion.max_end_speed, motion.max_acceleration);

            motion.max_start_speed = (motion.max_end_speed
                + ramp_down_time * motion.max_acceleration)
                .min(motion.max_speed);
            next_max_start_speed = motion.max_start_speed;
        }
    }

    pub fn next(&mut self, out: &mut Vec<LinearMotion>) {
        // TODO: Check if it is fully constrained yet.

        if let Some(motion) = self.queue.pop_front() {
            self.start_velocity = motion.calculate_motions(self.start_velocity.clone(), out);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn single_axis_path() {
        let mut planner = LinearMotionPlanner::new(Vector3f::zero());
        planner.move_to(Vector3f::from_slice(&[100.0, 0.0, 0.0]), 100.0, 1000.0);

        let mut out = vec![];
        planner.next(&mut out);
        println!("{:#?}", out);
    }

    #[test]
    fn single_axis_not_enough_time_to_speed_up() {
        let mut planner = LinearMotionPlanner::new(Vector3f::zero());
        planner.move_to(Vector3f::from_slice(&[100.0, 0.0, 0.0]), 100.0, 1.0);

        let mut out = vec![];
        planner.next(&mut out);
        println!("{:#?}", out);
    }

    #[test]
    fn works() {
        // 20 revolutions

        let mut planner = LinearMotionPlanner::new(Vector3f::zero());
        planner.move_to(Vector3f::from_slice(&[64000.0, 0.0, 0.0]), 3200.0, 500.0);

        let mut out = vec![];
        planner.next(&mut out);
        println!("{:#?}", out);
    }

    /*
    #[test]
    fn works() {
        let mut planner = LinearMotionPlanner::new(Vector3f::zero());

        planner.append(
            Vector3f::from_slice(&[0.0, 0.0, 0.0]),
            Vector3f::from_slice(&[100.0, 0.0, 0.0]),
            50.0,
            1000.0,
        );

        planner.append(
            Vector3f::from_slice(&[100.0, 0.0, 0.0]),
            Vector3f::from_slice(&[200.0, 0.0, 0.0]),
            50.0,
            1000.0,
        );

        let mut out = vec![];
        planner.next(&mut out);
        planner.next(&mut out);

        println!("{:#?}", out);
    }
    */
}

/*
We have linear motion constraints:
- Given geometry, convert to motor motions.
- Apply steps-per-mm and convert to integers

*/
