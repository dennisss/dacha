use std::convert::From;


/// Any number representable as a fraction of two integers.
/// Internally it is always stored 
pub struct Rational {
	upper: isize,
	lower: usize
}

impl From<isize> for Rational {
	fn from(v: isize) -> Self {

	}
}
