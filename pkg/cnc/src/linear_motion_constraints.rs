use alloc::vec::Vec;

use math::matrix::cwise_binary_ops::*;
use math::matrix::Vector3f;

use crate::displacement::*;
use crate::linear_motion::LinearMotion;

/// A non-fully defined LinearMotion(s).
///
/// While we know the exact start and end positions, this stores bounds on other
/// parameters like the traversal speeds.
///
/// This data structure is gradually refined by the LinearMotionPlanner. When we
/// are ready to convert it to motions, Self::calculate_motions will do that.
#[derive(Debug)]
pub struct LinearMotionConstraints {
    /// Position at which the motion will be started.
    ///
    /// TODO: Consider removing this since it will be redundant with the previous motion
    /// in the queue and is more data to maintain.
    pub start_position: Vector3f,

    /// Target end position after the motion is complete.
    pub end_position: Vector3f,

    /// Maximum speed at which we can end the motion.
    /// (we will try to optimize for finishing each motion at as fast a speed as possible).
    pub max_end_speed: f32,

    /// Overall max speed that can be hit during this motion.
    /// This value is a magnitude and should be >= 0.
    ///
    /// NOTE: This is a constant set by the GCode command's feedrate setting.
    pub max_speed: f32,

    /// Max acceleration at which we can move along the vector from 'start_position'
    /// to 'end_position'.
    pub max_acceleration: f32,
}

impl LinearMotionConstraints {

    pub fn is_empty(&self) -> bool {
        let distance_vector = &self.end_position - &self.start_position;
        distance_vector.norm() <= 1e-6
    }

    /// Given the motion constraints and the current start_velocity, generates a
    /// set of LinearMotions that go from start_position to end_position while
    /// satisfying all other constraints in as little time as possible.
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
        let mut start_speed = start_velocity.dot(&direction).max(0.0);

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
        // constant acceleration of +max_acceleration and then ramped down with a
        // constant acceleration of -max_acceleration to the end_speed.
        //
        // (this assumes there is no speed limit so no need to cruise)
        //
        // If start_speed == end_speed, then velocity would be a symetric triangle.
        let peak_speed = {
            // Extra time that we need to spend on ramping down vs. ramping up if end_speed
            // < start_speed.
            let k = (start_speed - end_speed) / self.max_acceleration;
            
            // Solving for the amount of time needed for ramp up 'x'. Ramp down will
            // take 'x + k' time.
            // 
            // See trapezoid.py for how this is calculated.
            let a = self.max_acceleration;
            let b = 2.0 * start_speed;
            let c = -self.max_acceleration * (k * k) / 2.0 - distance + k * start_speed;
            let (t1, t2) = math::find_quadratic_roots(a, b, c);

            // Note that I'm pretty sure that only one of these can be >= 0.0
            let rampup_time = {
                if t2 >= 0.0 && t1 >= 0.0 {
                    t2.min(t1)
                } else {
                    t2.max(t1)
                }
            };

            rampup_time * self.max_acceleration + start_speed
        };

        // Clamp
        let peak_speed = peak_speed.min(self.max_speed);

        // TODO: Immediately clamp these to zero here and add extra to the cruise if needed.
        let ramp_up_time = (peak_speed - start_speed) / self.max_acceleration;
        let ramp_down_time = (peak_speed - end_speed) / self.max_acceleration;

        let ramp_up_distance =
            displacement_traveled(start_speed, self.max_acceleration, ramp_up_time);
        let ramp_down_distance =
            displacement_traveled(peak_speed, -self.max_acceleration, ramp_down_time);

        let cruise_distance = distance - ramp_up_distance - ramp_down_distance;
        assert!(cruise_distance >= -0.001, "{:?}, start_velocity: {:?}", self, start_velocity);

        let cruise_time = cruise_distance / peak_speed;

        let mut current_position = self.start_position.clone();
        // This is start_velocity but with orthogonal components removed.
        let mut current_velocity = (&direction).cwise_mul(start_speed);

        // TODO: Better to threshold this on time since it is harder to handle having many small curves
        if ramp_up_distance.abs() >= 0.01 {
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

        if cruise_distance.abs() >= 0.01 {
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

        if ramp_down_distance.abs() >= 0.01 {
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

#[cfg(test)]
mod tests {

    use super::*;

    use math::matrix::vec3f;

    #[test]
    fn real_example() {
        let start_velocity = vec3f(16.762085, 0.0, 0.0);

        let c = LinearMotionConstraints { start_position: vec3f(3199.8596, 0.0, 0.0), end_position: vec3f(3200.0, 0.0, 0.0), max_end_speed: 0.0, max_speed: 200.0, max_acceleration: 1000.0 };

        // Mainly verifying it doesn't crash.
        let mut out = vec![];
        let end_velocity = c.calculate_motions(start_velocity, &mut out);
    }

    #[test]
    fn start_and_stop_at_rest() {

        // All three curves can be added.
        {
            let start_velocity = vec3f(0.0, 0.0, 0.0);

            let c = LinearMotionConstraints {
                start_position: vec3f(0.0, 0.0, 0.0),
                end_position: vec3f(1000.0, 0.0, 0.0),
                max_end_speed: 0.0,
                max_speed: 100.0,
                max_acceleration: 100.0,
            };

            let mut out = vec![];
            let end_velocity = c.calculate_motions(start_velocity, &mut out);

            assert_eq!(end_velocity, vec3f(0.0, 0.0, 0.0));

            assert_eq!(&out[..], &[
                LinearMotion {
                    start_position: vec3f(0., 0., 0.),
                    start_velocity: vec3f(0., 0., 0.),
                    end_position: vec3f(50., 0., 0.),
                    end_velocity: vec3f(100., 0., 0.),
                    acceleration: vec3f(100., 0., 0.),
                    duration: 1.0,
                },
                LinearMotion {
                    start_position: vec3f(50., 0., 0.),
                    start_velocity: vec3f(100., 0., 0.),
                    end_position: vec3f(950., 0., 0.),
                    end_velocity: vec3f(100., 0., 0.),
                    acceleration: vec3f(0., 0., 0.),
                    duration: 9.0,
                },
                LinearMotion {
                    start_position: vec3f(950., 0., 0.),
                    start_velocity: vec3f(100., 0., 0.),
                    end_position: vec3f(1000., 0., 0.),
                    end_velocity: vec3f(0., 0., 0.),
                    acceleration: vec3f(-100., 0., 0.),
                    duration: 1.0,
                },
            ][..]);
        }

        // Just enough time for two curves
        {
            let start_velocity = vec3f(0.0, 0.0, 0.0);

            let c = LinearMotionConstraints {
                start_position: vec3f(0.0, 0.0, 0.0),
                end_position: vec3f(100.0, 0.0, 0.0),
                max_end_speed: 0.0,
                max_speed: 100.0,
                max_acceleration: 100.0,
            };

            let mut out = vec![];
            let end_velocity = c.calculate_motions(start_velocity, &mut out);

            assert_eq!(end_velocity, vec3f(0.0, 0.0, 0.0));

            assert_eq!(&out[..], &[
                LinearMotion {
                    start_position: vec3f(0., 0., 0.),
                    start_velocity: vec3f(0., 0., 0.),
                    end_position: vec3f(50., 0., 0.),
                    end_velocity: vec3f(100., 0., 0.),
                    acceleration: vec3f(100., 0., 0.),
                    duration: 1.0,
                },
                LinearMotion {
                    start_position: vec3f(50., 0., 0.),
                    start_velocity: vec3f(100., 0., 0.),
                    end_position: vec3f(100., 0., 0.),
                    end_velocity: vec3f(0., 0., 0.),
                    acceleration: vec3f(-100., 0., 0.),
                    duration: 1.0,
                },
            ][..]);
        }

        // We have just enough space to get to top speed but shouldn't because we need to immediately start
        // slowing down.
        {
            let start_velocity = vec3f(0.0, 0.0, 0.0);

            let c = LinearMotionConstraints {
                start_position: vec3f(0.0, 0.0, 0.0),
                end_position: vec3f(50.0, 0.0, 0.0),
                max_end_speed: 0.0,
                max_speed: 100.0,
                max_acceleration: 100.0,
            };

            let mut out = vec![];
            let end_velocity = c.calculate_motions(start_velocity, &mut out);

            assert_eq!(end_velocity, vec3f(0.0, 0.0, 0.0));

            assert_eq!(&out[..], &[
                LinearMotion {
                    start_position: vec3f(0., 0., 0.),
                    start_velocity: vec3f(0., 0., 0.),
                    end_position: vec3f(25., 0., 0.),
                    end_velocity: vec3f(70.710677, 0., 0.),
                    acceleration: vec3f(100., 0., 0.),
                    duration: 0.70710677,
                },
                LinearMotion {
                    start_position: vec3f(25., 0., 0.),
                    start_velocity: vec3f(70.710677, 0., 0.),
                    end_position: vec3f(50., 0., 0.),
                    end_velocity: vec3f(0., 0., 0.),
                    acceleration: vec3f(-100., 0., 0.),
                    duration: 0.70710677,
                },
            ]);
        }

        // Similar to last case but we have a little more space.
        {
            let start_velocity = vec3f(0.0, 0.0, 0.0);

            let c = LinearMotionConstraints {
                start_position: vec3f(0.0, 0.0, 0.0),
                end_position: vec3f(80.0, 0.0, 0.0),
                max_end_speed: 0.0,
                max_speed: 100.0,
                max_acceleration: 100.0,
            };

            let mut out = vec![];
            let end_velocity = c.calculate_motions(start_velocity, &mut out);

            assert_eq!(end_velocity, vec3f(0.0, 0.0, 0.0));

            assert_eq!(&out[..], &[
                LinearMotion {
                    start_position: vec3f(0., 0., 0.),
                    start_velocity: vec3f(0., 0., 0.),
                    end_position: vec3f(40., 0., 0.),
                    end_velocity: vec3f(89.44272, 0., 0.),
                    acceleration: vec3f(100., 0., 0.),
                    duration: 0.8944272,
                },
                LinearMotion {
                    start_position: vec3f(40., 0., 0.),
                    start_velocity: vec3f(89.44272, 0., 0.),
                    end_position: vec3f(80., 0., 0.),
                    end_velocity: vec3f(0., 0., 0.),
                    acceleration: vec3f(-100., 0., 0.),
                    duration: 0.8944272,
                },
            ][..]);

            // println!("{:#?}", out);
        }

    }

    #[test]
    fn start_moving() {
        // Just cruising at start speed.
        {
            let start_velocity = vec3f(100.0, 0.0, 0.0);

            let c = LinearMotionConstraints {
                start_position: vec3f(0.0, 0.0, 0.0),
                end_position: vec3f(200.0, 0.0, 0.0),
                max_end_speed: 100.0,
                max_speed: 100.0,
                max_acceleration: 100.0,
            };

            let mut out = vec![];
            let end_velocity = c.calculate_motions(start_velocity, &mut out);

            assert_eq!(end_velocity, vec3f(100.0, 0.0, 0.0));

            assert_eq!(&out[..], &[
                LinearMotion {
                    start_position: vec3f(0., 0., 0.),
                    start_velocity: vec3f(100., 0., 0.),
                    end_position: vec3f(200., 0., 0.),
                    end_velocity: vec3f(100., 0., 0.),
                    acceleration: vec3f(0., 0., 0.),
                    duration: 2.0,
                },
            ][..]);

            // println!("{:#?}", out);
        }

        // Speed up then cruise
        {
            let start_velocity = vec3f(100.0, 0.0, 0.0);

            let c = LinearMotionConstraints {
                start_position: vec3f(0.0, 0.0, 0.0),
                end_position: vec3f(1000.0, 0.0, 0.0),
                max_end_speed: 200.0,
                max_speed: 200.0,
                max_acceleration: 100.0,
            };

            let mut out = vec![];
            let end_velocity = c.calculate_motions(start_velocity, &mut out);

            assert_eq!(end_velocity, vec3f(200.0, 0.0, 0.0));

            assert_eq!(&out[..], &[
                LinearMotion {
                    start_position: vec3f(0., 0., 0.),
                    start_velocity: vec3f(100., 0., 0.),
                    end_position: vec3f(150., 0., 0.),
                    end_velocity: vec3f(200., 0., 0.),
                    acceleration: vec3f(100., 0., 0.),
                    duration: 1.0,
                },
                LinearMotion {
                    start_position: vec3f(150., 0., 0.),
                    start_velocity: vec3f(200., 0., 0.),
                    end_position: vec3f(1000., 0., 0.),
                    end_velocity: vec3f(200., 0., 0.),
                    acceleration: vec3f(0., 0., 0.),
                    duration: 4.25,
                },
            ][..]);

            // println!("{:#?}", out);
        }
    }

    #[test]
    fn immediate_stop() {


        // Need to immediately slow down.
        {
            let start_velocity = vec3f(100.0, 0.0, 0.0);

            let c = LinearMotionConstraints {
                start_position: vec3f(0.0, 0.0, 0.0),
                end_position: vec3f(50.0, 0.0, 0.0),
                max_end_speed: 0.0,
                max_speed: 200.0,
                max_acceleration: 100.0,
            };

            let mut out = vec![];
            let end_velocity = c.calculate_motions(start_velocity, &mut out);

            assert_eq!(end_velocity, vec3f(0.0, 0.0, 0.0));

            assert_eq!(&out[..], &[
                LinearMotion {
                    start_position: vec3f(0., 0., 0.),
                    start_velocity: vec3f(100., 0., 0.),
                    end_position: vec3f(50., 0., 0.),
                    end_velocity: vec3f(0., 0., 0.),
                    acceleration: vec3f(-100., 0., 0.),
                    duration: 1.0,
                },
            ][..]);

            // println!("{:#?}", out);
        }
    }

    #[test]
    fn stop_soon() {

        {
            let start_velocity = vec3f(100.0, 0.0, 0.0);

            let c = LinearMotionConstraints {
                start_position: vec3f(0.0, 0.0, 0.0),
                end_position: vec3f(60.0, 0.0, 0.0),
                max_end_speed: 0.0,
                max_speed: 200.0,
                max_acceleration: 100.0,
            };

            let mut out = vec![];
            let end_velocity = c.calculate_motions(start_velocity, &mut out);

            assert_eq!(end_velocity, vec3f(0.0, 0.0, 0.0));

            // TODO: This needs a better comparator.
            /*
            assert_eq!(&out[..], &[
                LinearMotion {
                    start_position: vec3f(0., 0., 0.),
                    start_velocity: vec3f(100., 0., 0.),
                    end_position: vec3f(5., 0., 0.),
                    end_velocity: vec3f(104.8809, 0., 0.),
                    acceleration: vec3f(100., 0., 0.),
                    duration: 0.048808824,
                },
                LinearMotion {
                    start_position: vec3f(5., 0., 0.),
                    start_velocity: vec3f(104.8809, 0., 0.),
                    end_position: vec3f(60., 0., 0.),
                    end_velocity: vec3f(0., 0., 0.),
                    acceleration: vec3f(-100., 0., 0.),
                    duration: 1.0488088,
                },
            ][..]);
            */

            println!("{:#?}", out);
        }

        // TODO: Test with just a ramp up stage.

        // TODO: Test with starting at one non-zero speed and ending at another non-zero speed.


    }





}

