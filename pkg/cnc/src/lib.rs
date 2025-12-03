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