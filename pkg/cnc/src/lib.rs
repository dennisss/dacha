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