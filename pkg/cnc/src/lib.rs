#![no_std]

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[macro_use]
extern crate macros;

pub mod displacement;
#[cfg(feature = "alloc")]
pub mod linear_motion;
#[cfg(feature = "alloc")]
pub mod linear_motion_constraints;
#[cfg(feature = "alloc")]
pub mod linear_motion_planner;
pub mod kinematics;
pub mod quadratic_stepper_motion;
#[cfg(feature = "alloc")]
mod quadratic_interpolation;
#[cfg(feature = "alloc")]
pub mod stepping;
#[cfg(feature = "alloc")]
pub mod constrained_vector;


/// Computes 'next_time - current_time' assuming that that value
/// should be positive (will account for u32 wrapping).
pub fn time_remaining_u32(next_time: u32, current_time: u32) -> u32 {
    let mut t = next_time.wrapping_sub(current_time);
    if next_time < current_time {
        t = t.wrapping_add(u32::max_value());
    }

    t
}

/// Computes 'next_time - current_time'. This picks the smallest delta that makes
/// sense with integer wrapping.
pub fn time_difference_u32(next_time: u32, current_time: u32) -> i32 {
    // TODO: Optimize this.
   
    if next_time > current_time {
        return (next_time - current_time) as i32;
    } else {
        return -((current_time - next_time) as i32);
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn time_difference_u32_works() {

        assert_eq!(time_difference_u32(12, 12), 0);
        assert_eq!(time_difference_u32(101, 100), 1);
        assert_eq!(time_difference_u32(99, 100), -1);
        assert_eq!(time_difference_u32(1, 0xffffffff), 2);
        assert_eq!(time_difference_u32(0xffffffff, 1), -2);
        assert_eq!(time_difference_u32(0xffffffff, 100), -101);
        assert_eq!(time_difference_u32(0xffffffff, 0xfffffffe), 1);

    }


}