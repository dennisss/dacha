use typenum::Unsigned;
use std::ops::Mul;

/// A dimension cardinality (i.e. # of rows or columns).
/// This is essentially a wrapper around a usize value which may be a constant.
pub trait Dimension : Clone + Copy {
	/// If the dimension is fixed to a value at compile time, then this should
	/// return the size. Otherwise, it can return None.
	fn dim() -> Option<usize>;

	/// Returns the size of the dimension known at runtime. If dim() returns a
	/// non-None value, then this should return the same thing.
	fn value(&self) -> usize;

	/// Should create an instance of this type of dimension which will return
	/// the given value. If the type doesn't support the given value, then this
	/// function should panic.
	fn from_usize(value: usize) -> Self;
}

/// Marker trait for specifying that the given type must be a static fixed value
/// such as typenum::{U1, U2, U3, etc.}
pub trait StaticDim = Unsigned + Copy + Default;

impl<T: StaticDim> Dimension for T {
	fn dim() -> Option<usize> { Some(T::to_usize()) }
	fn value(&self) -> usize { T::to_usize() }
	fn from_usize(value: usize) -> Self {
		assert_eq!(value, T::to_usize());
		T::default()
	}
}

/// A simple dimension which is defined at runtime.
#[derive(Clone, Copy)]
pub struct Dynamic {
	value: usize
}

impl Dimension for Dynamic {
	fn dim() -> Option<usize> { None }
	fn value(&self) -> usize { self.value }
	fn from_usize(value: usize) -> Self {
		Self { value }
	}
}

/// Marker trait for specifying a Dynamic type dimension.
pub trait DynamicDim {}
impl DynamicDim for Dynamic {}


/// Trait implemented for pairs of dims which could be equal statically.
pub trait MaybeEqualDims {}
impl<S: Dimension> MaybeEqualDims for (S, S) {}
impl MaybeEqualDims for (Dynamic, Dynamic) {}
impl<S: StaticDim> MaybeEqualDims for (Dynamic, S) {}
impl<S: StaticDim> MaybeEqualDims for (S, Dynamic) {}


/// Any pair of Dimensions should be multipliable dynamically and statically.
/// At compile time, if the product isn't well defined, then the output type
/// will be Dynamic.
pub trait MulDims<Rhs>: Dimension {
	type Output: Dimension;
	fn mul_dims(self, rhs: Rhs) -> Self::Output;
}


impl<A: StaticDim + Mul<B>, B: StaticDim> MulDims<B> for A
where <A as Mul<B>>::Output: StaticDim {
	type Output = <A as Mul<B>>::Output;
	fn mul_dims(self, rhs: B) -> Self::Output {
		Self::Output::from_usize(self.value()*rhs.value())
	}
}

impl<S: StaticDim> MulDims<Dynamic> for S {
	type Output = Dynamic;
	fn mul_dims(self, rhs: Dynamic) -> Self::Output {
		Self::Output::from_usize(self.value()*rhs.value())
	}
}

impl<S: StaticDim> MulDims<S> for Dynamic {
	type Output = Dynamic;
	fn mul_dims(self, rhs: S) -> Self::Output {
		Self::Output::from_usize(self.value()*rhs.value())
	}
}


impl MulDims<Dynamic> for Dynamic {
	type Output = Dynamic;
	fn mul_dims(self, rhs: Dynamic) -> Self::Output {
		Self::Output::from_usize(self.value()*rhs.value())
	}
}

pub type ProdDims<A, B> = <A as MulDims<B>>::Output;
