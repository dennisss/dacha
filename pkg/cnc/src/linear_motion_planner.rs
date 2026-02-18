use alloc::{collections::VecDeque, vec::Vec};
use std::f64::consts::PI;

use math::matrix::cwise_binary_ops::*;
use math::matrix::VectorXd;
use math::matrix::StaticDim;
use math::vecxd;
use cnc_motion_proto::cnc::LinearMotionPlannerConfig;

use crate::displacement::*;
use crate::linear_motion::*;
use crate::linear_motion_constraints::*;
use crate::constrained_vector::*;

/// When chunking a long motion into smaller ones, this is the smallest chunk we will
/// allow rather than keeping the chunk merged with the previous chunk.
const MIN_RESIDUAL_MOTION_DURATION: f64 = 0.001; // 1ms


/// Plans a sequence of linear motions that are chained one
/// immediately after another in time.
///
/// Currently this implements cornering as follows:
/// - If there are at least 2 axes, the first two axes are assumed to be X/Y and the
///   config.max_junction_deviation parameter is used to compute the cornering speed.
///   - Any 180 degree turn or start/stop from/to zero velocity in the XY plane will
///     require a linear ramp up/down from zero velocity (no instant speed changes,
///     just rotations of the velocity vector are allowed)
/// - For all other axes, the 'config.max_instant_speed_change' parameter is used
///   to limit the speed.
///   - Starts/stops from rest set one of the endpoints to 0 speed.
///   - Motion in the same direction is not speed limited.
///   - A turn in the opposition direction is limited to entering the turn at
///     'config.max_instant_speed'.
///     - This is also accompanied by an instant speedup to
///       '-config.max_instant_speed' for the start of the next motion so that
///       the motion is symetric and minimized time around zero velocity.
pub struct LinearMotionPlanner  {
    config: LinearMotionPlannerConfig,
    start_time: f64,
    start_position: VectorXd,
    start_velocity: VectorXd,
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
    max_start_speed: f64,

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
    max_cornering_speed: f64,

    /// If true, max_start_velocity will no longer change if additional motions
    /// are added to this
    ///
    /// TODO: Get rid of this.
    fully_constrained: bool,
}

impl LinearMotionPlanner {
    pub fn new(start_position: VectorXd, config: LinearMotionPlannerConfig) -> Self {
        assert_eq!(config.max_instant_speed_change().len(), start_position.len());

        Self {
            start_time: 0.0,
            start_position: start_position.clone(),
            start_velocity: VectorXd::zero_with_shape(start_position.rows(), 1),
            queue: VecDeque::new(),
            config,
        }
    }

    fn zero_vec(&self) -> VectorXd {
        VectorXd::zero_with_shape(self.start_position.rows(), 1)
    }

    pub fn set_start_time(&mut self, v: f64) {
        self.start_time = v;
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.start_time = 0.0;
    }

    pub fn set_max_junction_deviation(&mut self, value: f64) {
        self.config.set_max_junction_deviation(value);
    }

    pub fn set_start_position(&mut self, start_position: VectorXd) {
        assert!(self.queue.is_empty());

        self.start_position = start_position.clone();
        self.start_velocity = VectorXd::zero_with_shape(start_position.rows(), 1);
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn last_position(&self) -> &VectorXd {
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
    pub fn move_to(&mut self, end_position: VectorXd, max_speed: f64, max_acceleration: f64) {
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
                cornering_accel,
                &self.config
            );

            last_motion.max_cornering_speed = max_cornering_speed
                .min(last_motion.constraints.max_speed);
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
        start_position: &VectorXd,
        corner_position: &VectorXd,
        next_position: &VectorXd,
        max_acceleration: f64,
        config: &LinearMotionPlannerConfig
    ) -> f64 {
        let mut overall_limits = vec![];

        let mut independent_axes_start = 0;

        // Junction deviation for X/Y since they are a coupled mass.
        if start_position.len() >= 2 {
            independent_axes_start = 2;

            let mut entry_delta = corner_position - start_position;
            let mut exit_delta = next_position - corner_position;

            // We only consider cornering over the first 2 axes.
            // Other axes are coordinated.
            // TODO: Need to make this behavior more configurable.
            for i in independent_axes_start..entry_delta.len() {
                entry_delta[i] = 0.0;
                exit_delta[i] = 0.0;
            }

            if entry_delta.norm() < 0.001 || exit_delta.norm() < 0.001 {
                // If there is no X/Y motion, then force a linear ramp to zero in those directions.

                for i in 0..start_position.len().min(2) {
                    overall_limits.push(0.0);
                }
            } else {
                let mut entry_direction = entry_delta.clone();
                let mut exit_direction = exit_delta.clone();

                entry_direction.normalize();
                exit_direction.normalize();

                let cornering_angle = (entry_direction.clone() * -1.0).dot(&exit_direction).acos();

                // Note: Divide by zero here will make the corner_radius 'inf'
                let corner_radius =
                    config.max_junction_deviation() * ((cornering_angle / 2.0).sin() / (1.0 - (cornering_angle / 2.0).sin()));

                let cornering_speed = (max_acceleration * corner_radius).sqrt()
                    // Getting rid of infinity 
                    .min(1000000.0)
                    // Speed for doing a 180 degree turn
                    .max(0.0);

                let cornering_speed_vec = entry_direction * cornering_speed;

                for i in 0..start_position.len().min(2) {
                    overall_limits.push(cornering_speed_vec[i].abs());
                }

            }

        }

        // Rest of axes are considered to be independent.
        for i in independent_axes_start..start_position.len() {
            let entry_delta = corner_position[i] - start_position[i];
            let exit_delta = next_position[i] - corner_position[i];

            if entry_delta.abs() < 0.0001 || exit_delta.abs() <= 0.001 {
                overall_limits.push(0.0);
                continue;
            }

            // Check if both directions have the same sign.
            if (entry_delta < 0.0) == (exit_delta < 0.0) {
                // No limit on exit speed.
                overall_limits.push(1000000.0);
            } else {
                // NOTE: With how many velocity rotation is implemented, this has the
                // effect of allowing this amount of speed for slowing down and this amount of
                // speed for speeding back up in the opposite direction (all instantaneously).
                overall_limits.push(config.max_instant_speed_change()[i]);
            }
        }

        constrained_vector(
            &(corner_position - start_position).normalized(),
            &overall_limits
        ).norm()
    }

    fn rotate_velocity(mut cur_velocity: VectorXd, dir: &VectorXd) -> VectorXd {
        let mut independent_axes_start = 0;

        if cur_velocity.len() >= 2 {
            independent_axes_start = 2;

            let xy_speed = (squared(cur_velocity[0]) + squared(cur_velocity[1])).sqrt();

            let mut vec = vecxd!(dir[0], dir[1]);
            vec.normalize();
            vec *= xy_speed;

            cur_velocity[0] = vec[0];
            cur_velocity[1] = vec[1];
        }

        // All the instant speed changes are symmetric.
        for i in independent_axes_start..cur_velocity.len() {
            if dir.norm() < 0.00001 {
                cur_velocity[i] = 0.0;
            } else {
                cur_velocity[i] = cur_velocity[i].copysign(dir[i]);
            }
        }

        cur_velocity
    }

    fn backpropagate_speed_limits(&mut self) {
        // Maximum velocity vector allowed when starting the next motion after the current one we are looking at.
        //
        // Initialized such that the final motion must end at rest.
        let mut next_max_start_velocity = self.zero_vec();

        // Iterate over motions in reverse order.
        for i in (0..self.queue.len()).rev() {
            let motion = &mut self.queue[i];

            let dir = (&motion.constraints.end_position - &motion.constraints.start_position).normalized();

            next_max_start_velocity = Self::rotate_velocity(next_max_start_velocity, &dir);

            let new_max_end_speed = constrained_vector(
                &dir,                
                // NOTE: This uses 'abs' to allow mining the speeds. Any change in direction
                // should already be taken care of above and in compute_max_cornering_speed.
                next_max_start_velocity.abs()
                    .cwise_min((dir.clone() * motion.max_cornering_speed).abs()).as_ref()
            ).norm();

            if (new_max_end_speed - motion.constraints.max_end_speed).abs() < 0.001 {
                break;
            }

            motion.constraints.max_end_speed = new_max_end_speed;

            // Amount of space in which we can accelerate/decelerate.
            // TODO: Dedup with the direction calculation.
            let distance = (&motion.constraints.end_position - &motion.constraints.start_position).norm();

            // Assuming we accelerated at the max allowed rate, how long would it take to
            // speed up/down from/to the end velocity while not overshooting the distance of
            // the linear motion.
            let ramp_down_time =
                time_to_travel(distance, motion.constraints.max_end_speed, motion.constraints.max_acceleration);

            motion.max_start_speed = (motion.constraints.max_end_speed
                + ramp_down_time * motion.constraints.max_acceleration)
                .min(motion.constraints.max_speed);
            next_max_start_velocity = dir * motion.max_start_speed;
        }
    }

    /// Calculates all available motions up to max_time.
    ///
    /// This may return slightly more time than requested to avoid making small motions
    /// (up to MIN_RESIDUAL_MOTION_DURATION more).
    pub fn next(
        &mut self,
        max_time: f64,
        out: &mut Vec<LinearMotion>
    ) {
        while self.start_time + MIN_RESIDUAL_MOTION_DURATION < max_time {
            let mut entry = match self.queue.pop_front() {
                Some(v) => v,
                None => break
            };

            // NOTE: This is safe under the assumption that previous motions followed the max
            // cornering speed rules such that the current velocity can be instantly
            // transformed in the new direction.
            let mut cur_velocity = {
                let dir = &entry.constraints.end_position - &entry.constraints.start_position;
                Self::rotate_velocity(self.start_velocity.clone(), &dir)
            };

            // TODO: FixedVec<3>
            let mut motions = vec![];
            let next_velocity = entry.constraints.calculate_motions(cur_velocity, &mut motions);

            for motion in motions {
                // Check if the current motion is too long to fit in the requested time window.
                // (in this case we need to put the overflowing time back in the queue).
                if self.start_time + motion.duration >= max_time + MIN_RESIDUAL_MOTION_DURATION {
                 
                    let mut t = (max_time - self.start_time).min(motion.duration);

                    // Worst case we split the motion into two parts each of size
                    // MIN_RESIDUAL_MOTION_DURATION
                    let (partial_motion, _) = motion.clone().split_at(t);

                    self.start_time += partial_motion.duration;
                    self.start_position = partial_motion.end_position.clone();
                    self.start_velocity = partial_motion.end_velocity.clone();
                    out.push(partial_motion);

                    // Push remaining parts of the motion back into the planner.
                    entry.constraints.start_position = self.start_position.clone();
                    self.queue.push_front(entry);
                    return;
                }

                self.start_time += motion.duration;
                self.start_position = motion.end_position.clone();
                self.start_velocity = motion.end_velocity.clone();
                out.push(motion);
            }
            
            if self.queue.len() > 0 {
                self.queue[0].constraints.start_position = self.start_position.clone();
            }
        }

        if self.start_time < max_time {
            self.start_time = max_time;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use math::vecxd;

    fn default_config() -> LinearMotionPlannerConfig {
        let mut config = LinearMotionPlannerConfig::default();
        config.set_max_junction_deviation(0.01);
        config
    }

    #[test]
    fn cornering() {

        let mut config = LinearMotionPlannerConfig::default();
        config.set_max_junction_deviation(0.2);
        config.add_max_instant_speed_change(0.0);
        config.add_max_instant_speed_change(0.0);
        config.add_max_instant_speed_change(0.0);

        println!("# straight (0 degrees)");
        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxd!(0.0, 0.0, 0.0),
            &vecxd!(1.0, 0.0, 0.0),
            &vecxd!(2.0, 0.0, 0.0),
            1000.0,
            &config,
        ));

        println!("# slight angle");
        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxd!(0.0, 0.0, 0.0),
            &vecxd!(1.0, 0.0, 0.0),
            &vecxd!(2.0, 0.2, 0.0),
            1000.0,
            &config,
        ));

        println!("# larger angle");
        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxd!(0.0, 0.0, 0.0),
            &vecxd!(1.0, 0.0, 0.0),
            &vecxd!(2.0, 0.6, 0.0),
            1000.0,
            &config,
        ));

        println!("# 45 degrees");
        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxd!(0.0, 0.0, 0.0),
            &vecxd!(1.0, 0.0, 0.0),
            &vecxd!(2.0, 1.0, 0.0),
            1000.0,
            &config,
        ));

        println!("# 90 degrees");
        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxd!(0.0, 0.0, 0.0),
            &vecxd!(1.0, 0.0, 0.0),
            &vecxd!(1.0, 1.0, 0.0),
            1000.0,
            &config,
        ));

        println!("# 180 degrees");
        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxd!(0.0, 0.0, 0.0),
            &vecxd!(1.0, 0.0, 0.0),
            &vecxd!(0.0, 0.0, 0.0),
            1000.0,
            &config,
        ));

        println!("# move then stop");
        println!("{}", LinearMotionPlanner::compute_max_cornering_speed(
            &vecxd!(0.0, 0.0, 0.0),
            &vecxd!(1.0, 0.0, 0.0),
            &vecxd!(1.0, 0.0, 0.0),
            1000.0,
            &config,
        ));
    }

    // motion_controller.move_to(vec3f(3600.0, 0.0, 0.0), 100.0).await?;

    #[test]
    fn split_motion_path() {
        let mut planner = LinearMotionPlanner::new(vecxd!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxd!(3600.0, 0.0, 0.0), 100.0, 1000.0);

        let mut out = vec![];
        planner.next(1000.0, &mut out);
        println!("{:#?}", out);

        // let mut out = vec![];
        // planner.next(1.0, 100, &mut out);
        // println!("{:#?}", out);
    }

    #[test]
    fn single_axis_path() {
        let mut planner = LinearMotionPlanner::new(vecxd!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxd!(100.0, 0.0, 0.0), 100.0, 1000.0);


        // let mut out = vec![];
        // planner.next(&mut out);
        // println!("{:#?}", out);
    }

    #[test]
    fn straight_line() {
        let mut planner = LinearMotionPlanner::new(vecxd!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxd!(100.0, 0.0, 0.0), 200.0, 1000.0);
        // Changing the speed so that these lines can't be merged.
        planner.move_to(vecxd!(110.0, 0.0, 0.0), 200.0, 1000.0);

        let mut out = vec![];
        planner.next(1000.0, &mut out);
        println!("{:#?}", out);
    }

    #[test]
    fn reverse_line() {
        let mut planner = LinearMotionPlanner::new(vecxd!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxd!(100.0, 0.0, 0.0), 100.0, 1000.0);
        planner.move_to(vecxd!(0.0, 0.0, 0.0), 100.0, 1000.0);

        let mut out = vec![];
        planner.next(1000.0, &mut out);
        println!("{:#?}", out);
    }



    #[test]
    fn single_axis_not_enough_time_to_speed_up() {
        let mut planner = LinearMotionPlanner::new(vecxd!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxd!(100.0, 0.0, 0.0), 100.0, 1.0);

        // let mut out = vec![];
        // planner.next(&mut out);
        // println!("{:#?}", out);
    }

    #[test]
    fn square_path() {
        let mut planner = LinearMotionPlanner::new(vecxd!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxd!(100.0, 0.0, 0.0), 100.0, 1000.0);
        planner.move_to(vecxd!(100.0, 100.0, 0.0), 100.0, 1000.0);
        planner.move_to(vecxd!(0.0, 100.0, 0.0), 100.0, 1000.0);
        planner.move_to(vecxd!(0.0, 0.0, 0.0), 100.0, 1000.0);

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

        let mut planner = LinearMotionPlanner::new(vecxd!(0.0, 0.0, 0.0), default_config());
        planner.move_to(vecxd!(64000.0, 0.0, 0.0), 3200.0, 500.0);

        // let mut out = vec![];
        // planner.next(&mut out);
        // println!("{:#?}", out);
    }

    /*
    #[test]
    fn works() {
        let mut planner = LinearMotionPlanner::new(vecxd!(0.0, 0.0, 0.0), default_config());

        planner.append(
            vecxd!(0.0, 0.0, 0.0),
            vecxd!(100.0, 0.0, 0.0),
            50.0,
            1000.0,
        );

        planner.append(
            vecxd!(100.0, 0.0, 0.0),
            vecxd!(200.0, 0.0, 0.0),
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


fn squared(v: f64) -> f64 {
    v * v
}

