#![feature(trait_alias, specialization)]

extern crate alloc;
extern crate core;

#[macro_use]
extern crate impl_ops;
extern crate generic_array;
extern crate num_traits;
extern crate typenum;
#[macro_use]
extern crate common;
#[macro_use]
extern crate approx;

pub mod argmax;
pub mod array;
pub mod assignment_solver;
pub mod big;
pub mod combin;
pub mod geometry;
pub mod matrix;
