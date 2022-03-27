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

/// Given parameters of an equation of the form 'Ax^2 + Bx + C = 0' finds values
/// of 'x' that satisfy the equation using the quadratic equation.
///
/// TODO: If we have a negative determinant, return None (or just return complex
/// numbers).
///
/// Returns the 2 roots. The first root is always >= the second root.
pub fn find_quadratic_roots(a: f32, b: f32, c: f32) -> (f32, f32) {
    let det = b * b - 4.0 * a * c;

    let root1 = (-b + det.sqrt()) / (2.0 * a);
    let root2 = (-b - det.sqrt()) / (2.0 * a);
    (root1, root2)
}
