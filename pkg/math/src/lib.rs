#![feature(trait_alias, const_fn, const_constructor, specialization)]
#[macro_use] extern crate arrayref;
#[macro_use] extern crate impl_ops;
extern crate num_traits;
extern crate typenum;
extern crate generic_array;
#[macro_use] extern crate common;

pub mod big;
pub mod array;
pub mod matrix;
pub mod assignment_solver;