#![no_std]

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

extern crate math;

pub mod kinematics;
pub mod linear_motion;
pub mod linear_motion_constraints;
pub mod linear_motion_planner;
