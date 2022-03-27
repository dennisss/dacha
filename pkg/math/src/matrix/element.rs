use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use num_traits::real::Real;
use num_traits::{One, Zero};

/// TODO: Eventually we should be using a relative percentage + absolute margin
/// based comparison.
pub trait ErrorEpsilon: Real {
    fn error_epsilon() -> Self;

    fn approx_zero(&self) -> bool {
        self.abs() < Self::error_epsilon()
    }
}

impl ErrorEpsilon for f64 {
    fn error_epsilon() -> Self {
        1e-12
    }
}

impl ErrorEpsilon for f32 {
    fn error_epsilon() -> Self {
        1e-12
    }
}

//impl ErrorEpsilon for usize {
//	fn error_epsilon() -> Self { 0 }
//}

// TODO: Should also read: https://eigen.tuxfamily.org/dox/TopicCustomizing_CustomScalar.html

/// Base type for all element types that can be used in a matrix. For simplicity
/// all matrices must be composed of a type that implements these traits.
pub trait ElementType = Copy + Default + Zero;

/// Traits expected by any scalar element of a matrix.
/// (i.e. real or complex number).
///
/// To simplify trait implementations, most trait implementations for numeric
/// calculations require these traits.
pub trait ScalarElementType = ElementType
    + Real
    + One
    + Add
    + AddAssign
    + Sub
    + SubAssign
    + Mul
    + MulAssign
    + Div
    + DivAssign
    + ErrorEpsilon;

// TODO: This will require floating point comparisons be possible with every
// type of scalar?
// const RELATIVE_MARGIN: f64 = 0.00001;

// pub fn approx_eq<T: ScalarElementType>(a: T, b: T) -> bool {

// }
