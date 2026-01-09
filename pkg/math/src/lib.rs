#![feature(
    trait_alias,
    specialization,
    generic_const_exprs,
    associated_type_defaults
)]
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
extern crate typenum;
#[macro_use]
extern crate common;
#[macro_use]
extern crate approx;

#[cfg(test)]
extern crate testing;

pub mod argmax;
#[cfg(feature = "alloc")]
pub mod array;
#[cfg(feature = "alloc")]
pub mod assignment_solver;
#[cfg(feature = "alloc")]
pub mod big;
pub mod combin;
pub mod gcd;
#[cfg(feature = "alloc")]
pub mod geometry;
pub mod integer;
pub mod intrinsics;
pub mod matrix;
pub mod number;
pub mod rational;
#[cfg(feature = "std")]
pub mod complex;

// TODO: Verify this uses hardware instructions on ARM.
pub use integer::Integer;
pub use intrinsics::*;
pub use number::Float;

/// Given parameters of an equation of the form 'Ax^2 + Bx + C = 0' finds values
/// of 'x' that satisfy the equation using the quadratic equation.
///
/// TODO: If we have a negative determinant, return None (or just return complex
/// numbers).
///
/// Returns the 2 roots. The first root is always >= the second root.
pub fn find_quadratic_roots<T: Float + matrix::element::ErrorEpsilon>(a: T, b: T, c: T) -> (T, T) {
    // TODO: Use approximate comparison
    if a.approx_zero() {
        let r = -c / b;
        return (r, r);
    }

    let det = b * b - T::from(4.0) * a * c;
    let det_root = det.sqrt();

    let two = T::from(2.0);
    let root1 = (-b + det_root) / (two * a);
    let root2 = (-b - det_root) / (two * a);
    (root1, root2)
}
