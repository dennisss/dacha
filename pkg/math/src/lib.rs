#![feature(trait_alias, specialization)]
#![no_std]

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[macro_use]
extern crate auto_ops;
extern crate generic_array;
extern crate num_traits;
extern crate typenum;
#[macro_use]
extern crate common;
#[macro_use]
extern crate approx;

pub mod argmax;
#[cfg(feature = "alloc")]
pub mod array;
#[cfg(feature = "alloc")]
pub mod assignment_solver;
#[cfg(feature = "alloc")]
pub mod big;
pub mod combin;
#[cfg(feature = "alloc")]
pub mod geometry;
pub mod matrix;

use num_traits::real::Real;

/// Given parameters of an equation of the form 'Ax^2 + Bx + C = 0' finds values
/// of 'x' that satisfy the equation using the quadratic equation.
///
/// TODO: If we have a negative determinant, return None (or just return complex
/// numbers).
///
/// Returns the 2 roots. The first root is always >= the second root.
pub fn find_quadratic_roots(a: f32, b: f32, c: f32) -> (f32, f32) {
    // TODO: Use approximate comparison
    if a == 0.0 {
        let r = -c / b;
        return (r, r);
    }

    let det = b * b - 4.0 * a * c;

    let root1 = (-b + det.sqrt()) / (2.0 * a);
    let root2 = (-b - det.sqrt()) / (2.0 * a);
    (root1, root2)
}
