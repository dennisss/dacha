#![feature(trait_alias, specialization)]
#[macro_use]
extern crate arrayref;
#[macro_use]
extern crate impl_ops;
extern crate generic_array;
extern crate num_traits;
extern crate typenum;
#[macro_use]
extern crate common;
#[macro_use]
extern crate approx;

pub mod array;
pub mod assignment_solver;
pub mod big;
pub mod combin;
pub mod geometry;
pub mod matrix;
