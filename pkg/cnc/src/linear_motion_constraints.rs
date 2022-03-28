use alloc::vec::Vec;

use math::matrix::cwise_binary_ops::*;
use math::matrix::Vector3f;

use crate::kinematics::*;
use crate::linear_motion::LinearMotion;

/// A non-fully defined LinearMotion(s).
///
/// While we know the exact start and end positions, this stores bounds on other
/// parameters like the traversal speeds.
///
/// This data structure is gradually refined by the LinearMotionPlanner. When we
/// are ready to convert it to motions, Self::calculate_motions will do that.
pub struct LinearMotionConstraints {
    pub start_position: Vector3f,

    pub end_position: Vector3f,

    /// Maximum velocity magnitude at which we can start this motion such the
    /// velocity can be safely reduced using this motion's acceleration to
    /// max(this.max_cornering_speed, next_motion.max_start_speed).
    ///
    /// This also can't be higher than this.max_speed.
    ///
    /// NOTE: If there is no motion following this one, then the above max(...)
    /// expression is tentatively 0. As such, this number can change as new
    /// motions are added.
    pub max_start_speed: f32,

    pub max_end_speed: f32,

    /// Overall max speed that can be hit during this motion.
    /// This value is a magnitude and should be >= 0.
    ///
    /// NOTE: This is a constant set by the GCode command's feedrate setting.
    pub max_speed: f32,

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
    pub max_cornering_speed: f32,

    /// Max acceleration at which we can change each axis's velocity.
    pub max_acceleration: f32,

    /// If true, max_start_velocity will no longer change if additional motions
    /// are added to this
    pub fully_constrained: bool,
}

impl LinearMotionConstraints {
    /// Given the motion constraints and the current start_velocity, generates a
    /// set of LinearMotions that go from start_position to end_position while
    /// satisfying all other constraints in all little time as possible.
    ///
    /// This will generate up to 3 motions:
    /// 1. A ramp up at constant positive acceleration to get to some peak
    ///    velocity.
    ///    - Note: We assume that the start_velocity is <= max_start_velocity.
    /// 2. A cruising phase at the peak velocity with zero acceleration.
    /// 3. A ramp down at constant negative acceleration to get to an end speed
    ///    suitable for transitioning to the next motion.
    ///
    /// We solve this optimization problem by:
    /// 1. Determining the exact end_speed we want to hit based on the maximum
    ///    possible speed reachable with pure acceleration.
    /// 2. Solving for the peak speed to use for the cruising phase.
    ///    - We indirectly solve for this by solving for the time ('x') spent
    ///      ramping up to the peak speed.
    ///    - start_speed and end_speed are known.
    ///    - The time spent ramping down should be symmetric to the time spent
    ///      ramping up speed so will be 'x + k' where 'k' can be derived from
    ///      the difference between start_speed and end_speed.
    ///
    /// Returns the new velocity after the motions are complete.
    pub fn calculate_motions(
        &self,
        start_velocity: Vector3f,
        out: &mut Vec<LinearMotion>,
    ) -> Vector3f {
        let distance_vector = &self.end_position - &self.start_position;
        if distance_vector.norm() <= 1e-6 {
            return start_velocity;
        }

        let distance = distance_vector.norm();
        let direction = distance_vector.normalized();

        // If we are traveling in a different direction initially, assume we can
        // instantly stop (no ramp downs in velocity as added at the start of the
        // motion).
        let mut start_speed = start_velocity.dot(&direction);
        if start_speed < 0.0 {
            start_speed = 0.0;
        }

        let end_speed = {
            if self.max_end_speed <= start_speed {
                self.max_end_speed
            } else {
                // End speed is allowed to go above initial speed.
                // See how fast we can go if we do nothing but ramp up speed at the max
                // acceleration.
                let time = time_to_travel(distance, start_speed, self.max_acceleration);
                let largest_possible_end_speed = start_speed + time * self.max_acceleration;

                largest_possible_end_speed.min(self.max_end_speed)
            }
        };

        // Compute the maximum velocity we can reach if we simply used a
        // constant velocity of +max_acceleration and then ramped down with a
        // constant acceleration of -max_acceleration to the end_speed.
        //
        // If start_speed == end_speed, then velocity would be a symetric triangle.
        let peak_speed = {
            // Extra time that we need to spend on ramping down vs. ramping up if end_speed
            // < start_speed.
            let k = (start_speed - end_speed) / self.max_acceleration;

            // Solving quadratic equation formulated using 'x' as the amount of time spent
            // ramping up to the peak speed and 'x + k' being the amount of time ramping
            // down.

            // 0 = X**2*acceleration + (2*start_speed - K*acceleration) * X +
            // -K**2*acceleration/2 - distance
            let a = self.max_acceleration;
            let b = 2.0 * start_speed - k * self.max_acceleration;
            let c = -1.0 * k * k * self.max_acceleration / 2.0 - distance;

            let (t1, t2) = math::find_quadratic_roots(a, b, c);
            // TODO: Check this.
            let rampup_time = {
                if t2 >= 0.0 && t1 >= 0.0 {
                    t2.min(t1)
                } else if t2 >= 0.0 {
                    t2
                } else {
                    t1
                }
            };

            let absolute_peak_speed = rampup_time * self.max_acceleration + start_speed;

            absolute_peak_speed.min(self.max_speed)
        };

        let ramp_up_time = (peak_speed - start_speed) / self.max_acceleration;
        let ramp_down_time = (peak_speed - end_speed) / self.max_acceleration;

        let ramp_up_distance =
            displacement_traveled(start_speed, self.max_acceleration, ramp_up_time);
        let ramp_down_distance =
            displacement_traveled(peak_speed, -self.max_acceleration, ramp_down_time);

        let cruise_distance = distance - ramp_up_distance - ramp_down_distance;
        assert!(cruise_distance >= 0.0);

        let cruise_time = cruise_distance / peak_speed;

        let mut current_position = self.start_position.clone();
        // This is start_velocity but with orthogonal components removed.
        let mut current_velocity = (&direction).cwise_mul(start_speed);

        if ramp_up_distance >= 0.01 {
            let start_position = current_position.clone();
            let end_position = &start_position + (&direction).cwise_mul(ramp_up_distance);
            current_position = end_position.clone();

            let acceleration = (&direction).cwise_mul(self.max_acceleration);

            let start_velocity = current_velocity.clone();
            let end_velocity = &start_velocity + (&acceleration).cwise_mul(ramp_up_time);
            current_velocity = end_velocity.clone();

            out.push(LinearMotion {
                start_position,
                start_velocity,
                end_position,
                end_velocity,
                acceleration,
                duration: ramp_up_time,
            });
        }

        if cruise_distance >= 0.01 {
            let start_position = current_position.clone();
            let end_position = &start_position + (&direction).cwise_mul(cruise_distance);
            current_position = end_position.clone();

            out.push(LinearMotion {
                start_position,
                start_velocity: current_velocity.clone(),
                end_position,
                end_velocity: current_velocity.clone(),
                acceleration: Vector3f::zero(),
                duration: cruise_time,
            });
        }

        if ramp_down_distance >= 0.01 {
            let start_position = current_position.clone();
            let end_position = &start_position + (&direction).cwise_mul(ramp_down_distance);
            current_position = end_position.clone();

            let acceleration = (&direction).cwise_mul(-self.max_acceleration);

            let start_velocity = current_velocity.clone();
            let end_velocity = &start_velocity + (&acceleration).cwise_mul(ramp_down_time);
            current_velocity = end_velocity.clone();

            out.push(LinearMotion {
                start_position,
                start_velocity,
                end_position,
                end_velocity,
                acceleration,
                duration: ramp_down_time,
            });
        }

        // TODO: Ensure at least one motion is always added.

        current_velocity
    }
}
