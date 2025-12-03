use alloc::{collections::VecDeque, vec::Vec};
use std::f32::consts::PI;

use math::matrix::cwise_binary_ops::*;
use math::matrix::VectorXf;
use math::matrix::StaticDim;
use cnc_motion_proto::cnc::LinearMotionPlannerConfig;

use crate::displacement::*;
use crate::linear_motion::*;
use crate::linear_motion_constraints::*;
use crate::constrained_vector::*;


/// Plans a sequence of linear motions that are chained one
/// immediately after another in time.
pub struct LinearMotionPlanner  {
    config: LinearMotionPlannerConfig,
    start_position: VectorXf,
    start_velocity: VectorXf,
    queue: VecDeque<LinearMotionQueueEntry>,
}

struct LinearMotionQueueEntry {
    constraints: LinearMotionConstraints,
 
    /// Maximum velocity magnitude at which we can start this motion such the
    /// velocity can be safely reduced using this motion's acceleration to
    /// max(this.max_cornering_speed, next_motion.max_start_speed).
    ///
    /// This also can't be higher than this.max_speed.
    ///
    /// NOTE: If there is no motion following this one, then the above max(...)
    /// expression is tentatively 0. As such, this number can change as new
    /// motions are added.
    max_start_speed: f32,

    /// Maximum velocity at which we can exit this motion based on the sharpness
    /// of the transition to the next motion.
    ///
    /// All values are >= 0.
    ///
    /// This is at most max_velocity when the next motion is in the same
    /// direction as the current motion and can reach zero if the next motion is
    /// in the opposite direction.
    ///
    /// This is initially 0 to imply that the final motion should bring us to a
    /// stop and set to a higher value when the next motion is appended to the
    /// plan.
        max_cornering_speed: f32,

    /// If true, max_start_velocity will no longer change if additional motions
    /// are added to this
    ///
    /// TODO: Get rid of this.
    fully_constrained: bool,
}

impl LinearMotionPlanner {
    pub fn new(start_position: VectorXf, config: LinearMotionPlannerConfig) -> Self {
        Self {
            start_position: start_position.clone(),
            start_velocity: VectorXf::zero_with_shape(start_position.rows(), 1),
            queue: VecDeque::new(),
            config,
        }
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }

    pub fn set_start_position(&mut self, start_position: VectorXf) {
        assert!(self.queue.is_empty());

        self.start_position = start_position.clone();
        self.start_velocity = VectorXf::zero_with_shape(start_position.rows(), 1);
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn last_position(&self) -> &VectorXf {
        match self.queue.back() {
            Some(v) => &v.constraints.end_position,
            None => &self.start_position
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
    // TODO: Need to combine motions in basically the same direction.
    //
    // TODO: If there are extremely long linear motions, split them into pieces so
    // that te planner can emit partial results quickly (similarly combine many
    // short movements in the same direction).
    pub fn move_to(&mut self, end_position: VectorXf, max_speed: f32, max_acceleration: f32) {
        let start_position = {
            if let Some(last_motion) = self.queue.back() {
                last_motion.constraints.end_position.clone()
            } else {
                self.start_position.clone()
            }
        };

        // TODO: Verify no discontinuity of positions.

        // TODO: When switching from a Z move to an X-Y move, should we require Z to reach zero
        // speed before switching?

        // If we had a previous motion, compute the max cornering speed.
        if let Some(last_motion) = self.queue.back_mut() {
            // See https://onehossshay.wordpress.com/2011/09/24/improving_grbl_cornering_algorithm/

            let cornering_accel = max_acceleration.min(last_motion.constraints.max_acceleration);

                        let mut max_cornering_speed = Self::compute_max_cornering_speed(
                &last_motion.constraints.start_position,
                &last_motion.constraints.end_position,
                &end_position,
                self.config.max_junction_deviation(),
                cornering_accel,
            );

            last_motion.max_cornering_speed = max_cornering_speed
                .min(last_motion.constraints.max_speed)
                .min(max_speed);

            // println!("Corner: {}", last_motion.max_cornering_speed);
        }

        // Append to queue.
        self.queue.push_back(LinearMotionQueueEntry {
            constraints: LinearMotionConstraints {
                start_position,
                end_position,
                max_end_speed: -1.0, // Force it to update during backpropagation.
                max_speed,
                max_acceleration,
            },
            max_start_speed: 0.0,
            max_cornering_speed: 0.0,
            fully_constrained: false,
        });

        self.backpropagate_speed_limits();
    }

        fn compute_max_cornering_speed(
        start_position: &VectorXf,
        corner_position: &VectorXf,
        next_position: &VectorXf,
        max_deviation: f32,
        max_acceleration: f32,
    ) -> f32 {
        /*
        Generalized cornering:
        - For X/Y, use corner radius


        - For Z use max jerk.
        - If any group stops moving across the turn, then cornering is zero.


        */

        let mut overall_limits = vec![];

        // Junction deviation for X/Y since they are a coupled mass.
        {
            let entry_delta = corner_position - start_position;
            let exit_delta = next_position - corner_position;

            if entry_delta.norm() < 0.001 || exit_delta.norm() < 0.001 {
                for i in 0..start_position.len().min(2) {
                    overall_limits.push(0.0);
                }
            } else {
                let mut entry_direction = entry_delta.clone();
                let mut exit_direction = exit_delta.clone();
                // We only consider cornering over the first 3 axes.
                // Other axes are coordinated.
                // TODO: Need to make this behavior more configurable.
                for i in 2..entry_delta.len() {
                    entry_direction[i] = 0.0;
                    exit_direction[i] = 0.0;
                }
                entry_direction.normalize();
                exit_direction.normalize();

                let cornering_angle = (entry_direction.clone() * -1.0).dot(&exit_direction).acos();

        // Note: Divide by zero here will make the corner_radius 'inf'
        let corner_radius =
            max_deviation * ((cornering_angle / 2.0).sin() / (1.0 - (cornering_angle / 2.0).sin()));

        let cornering_speed = (max_acceleration * corner_radius).sqrt()
                    .min(1000000.0);

let cornering_speed_vec = entry_direction * cornering_speed;

                for i in 0..start_position.len().min(2) {
                    overall_limits.push(cornering_speed_vec[i]);
                }

            }

        }

        // Rest of axes are considered to be independent.
        for i in 2..start_position.len() {
            let entry_delta = corner_position[i] - start_position[i];
            let exit_delta = next_position[i] - corner_position[i];

            if entry_delta < 0.0001 || exit_delta <= 0.001 {
                overall_limits.push(0.0);
                continue;
            }

            // Check if both directions have the same sign.
            if (entry_delta < 0.0) == (exit_delta < 0.0) {
                // No limit on exit speed.
                overall_limits.push(1000000.0);
            } else {
                // TODO: Allow a max instantaneous speed change?
                overall_limits.push(0.0);
            }
        }

        constrained_vector(
            &(corner_position - start_position).normalized(),
            &overall_limits
        ).norm()
    }

    fn backpropagate_speed_limits(&mut self) {
        // The final motion must end at rest.
        let mut next_max_start_speed: f32 = 0.0;

        for i in (0..self.queue.len()).rev() {
            let motion = &mut self.queue[i];

            let new_max_end_speed = next_max_start_speed.max(motion.max_cornering_speed);
            if (new_max_end_speed - motion.constraints.max_end_speed).abs() < 0.001 {
                break;
            }

            motion.constraints.max_end_speed = new_max_end_speed;

            // Amount of space in which we can accelerate/decelerate.
            let distance = (&motion.constraints.end_position - &motion.constraints.start_position).norm();

            // Assuming we accelerated at the max allowed rate, how long would it take to
            // speed up/down from/to the end velocity while not overshooting the distance of
            // the linear motion.
            let ramp_down_time =
                time_to_travel(distance, motion.constraints.max_end_speed, motion.constraints.max_acceleration);

            motion.max_start_speed = (motion.constraints.max_end_speed
                + ramp_down_time * motion.constraints.max_acceleration)
                .min(motion.constraints.max_speed);
            next_max_start_speed = motion.max_start_speed;
        }
    }

    /// Calculates the next 'n' linear motions according to the planned steps so far.
    ///
    /// We will stop once we either give back 'max_duration' total of motions or
    /// 'max_count' total motions.
    pub fn next(
        &mut self,
        max_duration: f32,
        max_count: usize,
        out: &mut Vec<LinearMotion>
    ) {
        if max_count == 0 || max_duration <= 0.0001 {
            return;
        }

        let mut dur = 0.0;
        let mut n = 0;

        while n < max_count && dur < max_duration {
            let mut entry = match self.queue.pop_front() {
                Some(v) => v,
                None => return
            };

            // TODO: FixedVec<3>
            let mut motions = vec![];
            let next_velocity = entry.constraints.calculate_motions(self.start_velocity.clone(), &mut motions);

            for motion in motions {

                if dur + motion.duration >= max_duration + 0.0001 || n == max_count {
                 
                    let mut t = (max_duration - dur).min(motion.duration);
                    if n == max_count {
                        t = 0.0;
                    }

                    let (partial_motion, _) = motion.split_at(t);

                    // TODO: Robustify this.
                    if partial_motion.duration >= 0.0001 {
                        self.start_position = partial_motion.end_position.clone();
                        self.start_velocity = partial_motion.end_velocity.clone();
                        out.push(partial_motion);
                    }

                    entry.constraints.start_position = self.start_position.clone();

                    // TODO: If it is not pushed, we should do a minor correction next time to 
                    // correctly align the next motion's start_position.
                    if !entry.constraints.is_empty() {
                        self.queue.push_front(entry);
                    }

                    return;
                }

                dur += motion.duration;
                n += 1;
                self.start_position = motion.end_position.clone();
                self.start_velocity = motion.end_velocity.clone();
                out.push(motion);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use math::vecxf;

    fn default_config() -> LinearMotionPlannerConfig {
        let mut config = LinearMotionPlannerConfig::default();
        config.set_max_junction_deviation(0.01);
        config
    }

    #[test]
    fn cornering() {

        let radius = 0.2;

        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxf!(0.0, 0.0, 0.0),
            &vecxf!(1.0, 0.0, 0.0),
            &vecxf!(2.0, 0.0, 0.0),
            radius,
            1000.0
        ));

        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxf!(0.0, 0.0, 0.0),
            &vecxf!(1.0, 0.0, 0.0),
            &vecxf!(2.0, 0.2, 0.0),
            radius,
            1000.0
        ));

        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxf!(0.0, 0.0, 0.0),
            &vecxf!(1.0, 0.0, 0.0),
            &vecxf!(2.0, 0.6, 0.0),
            radius,
            1000.0
        ));

        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxf!(0.0, 0.0, 0.0),
            &vecxf!(1.0, 0.0, 0.0),
            &vecxf!(2.0, 1.0, 0.0),
            radius,
            1000.0
        ));

        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxf!(0.0, 0.0, 0.0),
            &vecxf!(1.0, 0.0, 0.0),
            &vecxf!(1.0, 1.0, 0.0),
            radius,
            1000.0
        ));

        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxf!(0.0, 0.0, 0.0),
            &vecxf!(1.0, 0.0, 0.0),
            &vecxf!(0.0, 0.0, 0.0),
            radius,
            1000.0
        ));

        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxf!(0.0, 0.0, 0.0),
            &vecxf!(1.0, 0.0, 0.0),
            &vecxf!(1.0, 0.0, 0.0),
            0.2,
            1000.0
        ));
    }

    // motion_controller.move_to(vec3f(3600.0, 0.0, 0.0), 100.0).await?;

    #[test]
    fn split_motion_path() {
        let mut planner = LinearMotionPlanner::new(vecxf!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxf!(3600.0, 0.0, 0.0), 100.0, 1000.0);

        let mut out = vec![];
        planner.next(1000.0, 100, &mut out);
        println!("{:#?}", out);

        // let mut out = vec![];
        // planner.next(1.0, 100, &mut out);
        // println!("{:#?}", out);
    }

    #[test]
    fn single_axis_path() {
        let mut planner = LinearMotionPlanner::new(vecxf!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxf!(100.0, 0.0, 0.0), 100.0, 1000.0);


        // let mut out = vec![];
        // planner.next(&mut out);
        // println!("{:#?}", out);
    }

    #[test]
    fn straight_line() {
        let mut planner = LinearMotionPlanner::new(vecxf!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxf!(100.0, 0.0, 0.0), 200.0, 1000.0);
        // Changing the speed so that these lines can't be merged.
        planner.move_to(vecxf!(110.0, 0.0, 0.0), 200.0, 1000.0);

        let mut out = vec![];
        planner.next(1000.0, 1000, &mut out);
        println!("{:#?}", out);
    }

    #[test]
    fn reverse_line() {
        let mut planner = LinearMotionPlanner::new(vecxf!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxf!(100.0, 0.0, 0.0), 100.0, 1000.0);
        planner.move_to(vecxf!(0.0, 0.0, 0.0), 100.0, 1000.0);

        let mut out = vec![];
        planner.next(1000.0, 1000, &mut out);
        println!("{:#?}", out);
    }



    #[test]
    fn single_axis_not_enough_time_to_speed_up() {
        let mut planner = LinearMotionPlanner::new(vecxf!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxf!(100.0, 0.0, 0.0), 100.0, 1.0);

        // let mut out = vec![];
        // planner.next(&mut out);
        // println!("{:#?}", out);
    }

    #[test]
    fn square_path() {
        let mut planner = LinearMotionPlanner::new(vecxf!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxf!(100.0, 0.0, 0.0), 100.0, 1000.0);
        planner.move_to(vecxf!(100.0, 100.0, 0.0), 100.0, 1000.0);
        planner.move_to(vecxf!(0.0, 100.0, 0.0), 100.0, 1000.0);
        planner.move_to(vecxf!(0.0, 0.0, 0.0), 100.0, 1000.0);

        // let mut out = vec![];
        // planner.next(&mut out);
        // planner.next(&mut out);
        // planner.next(&mut out);
        // planner.next(&mut out);
        // println!("{:#?}", out);
    }

    #[test]
    fn works() {
        // 20 revolutions

        let mut planner = LinearMotionPlanner::new(vecxf!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxf!(64000.0, 0.0, 0.0), 3200.0, 500.0);

        // let mut out = vec![];
        // planner.next(&mut out);
        // println!("{:#?}", out);
    }

    /*
    #[test]
    fn works() {
        let mut planner = LinearMotionPlanner::new(vecxf!(0.0, 0.0, 0.0), default_config());

        planner.append(
            vecxf!(0.0, 0.0, 0.0),
            vecxf!(100.0, 0.0, 0.0),
            50.0,
            1000.0,
        );

        planner.append(
            vecxf!(100.0, 0.0, 0.0),
            vecxf!(200.0, 0.0, 0.0),
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
