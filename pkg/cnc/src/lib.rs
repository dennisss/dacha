#![no_std]

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

extern crate math;
#[macro_use]
extern crate macros;
extern crate protobuf;

pub mod kinematics;
pub mod linear_motion;
#[cfg(feature = "alloc")]
pub mod linear_motion_constraints;
#[cfg(feature = "alloc")]
pub mod linear_motion_planner;
pub mod proto;
