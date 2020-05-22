use std::ops::{Index, IndexMut, Mul};
use std::marker::PhantomData;
use typenum::Prod;
use generic_array::{GenericArray, ArrayLength};
use crate::matrix::dimension::*;
use crate::matrix::element::ElementType; // TODO: Refactor out this circular reference.


// trait Iteratable<T> {
// 	fn iter(&self) -> Iterator<Item=T>;
// }
// impl<T> Iteratable<T> for Vec<T> { fn iter(&self) -> Iterator<Item=T> { self.iter() } }


/// A container of elements. Must be indexable in natural contiguous order. Row
/// and column semantics will be defined at a higher level, so the storage type
/// should be agnostic to that.
/// 
/// There are three main StorageType implementations to be aware of:
/// - Vec: Dynamic contiguous container for a matrix
/// - MatrixInlineStorage: For statically known shapes, stores all elements on
///   the stack with zero allocations.
/// - MatrixBlockStorage: Doesn't directly own any elements, but instead stores
///   a reference to a block of elements in another storage type. 
pub trait StorageType<T>: Index<usize, Output=T> {
	fn alloc(size: usize) -> Self;
	fn as_ptr(&self) -> *const T; // NOTE: This is only a good thing to do for non-blocky situations.
}

pub trait StorageTypeMut<T> = StorageType<T> + IndexMut<usize>;

impl<T: ElementType> StorageType<T> for Vec<T> {
	fn alloc(size: usize) -> Self {
		let mut out = vec![];
		out.resize(size, T::zero());
		out
	}

	fn as_ptr(&self) -> *const T {
		AsPtr::as_ptr(self)
	}
}


pub trait AsPtr<T> {
	fn as_ptr(&self) -> *const T;
}

impl<T> AsPtr<T> for Vec<T> {
	fn as_ptr(&self) -> *const T {
		self.as_ptr()
	}
}

/// Storage which stores all elements in stack memory for statically
/// allocatable sizes.
#[derive(Clone)]
#[repr(packed)]
pub struct MatrixInlineStorage<T, N: ArrayLength<T>> {
	data: GenericArray<T, N>
}

impl<T, N: ArrayLength<T>>
AsRef<[T]> for MatrixInlineStorage<T, N> {
	fn as_ref(&self) -> &[T] { self.data.as_ref() }
}

impl<T, N: ArrayLength<T>>
AsMut<[T]> for MatrixInlineStorage<T, N> {
	fn as_mut(&mut self) -> &mut [T] { self.data.as_mut() }
}

impl<T, N: ArrayLength<T>>
std::ops::Index<usize> for MatrixInlineStorage<T, N> {
	type Output = T;
	fn index(&self, idx: usize) -> &T { &self.data.as_ref()[idx] }
}

impl<T, N: ArrayLength<T>>
std::ops::IndexMut<usize> for MatrixInlineStorage<T, N> {
	fn index_mut(&mut self, idx: usize) -> &mut T {
		&mut self.data.as_mut()[idx]
	}
}

impl<T: Default, N: ArrayLength<T>>
StorageType<T> for MatrixInlineStorage<T, N> {
	fn alloc(size: usize) -> Self {
		assert_eq!(N::to_usize(), size);
		Self {
			data: GenericArray::default()
		}
	}

	fn as_ptr(&self) -> *const T {
		self.data.as_ptr()
	}
}



/// A container of referenced elements used to represent a chunk of another
/// matrix inplace.
pub struct MatrixBlockStorage<'a, T, Tp: AsRef<[T]> + 'a, W: Dimension,
							  S: Dimension> {
	// TODO: Eventually make all fields here private.

	pub data: Tp,

	/// Number of contiguous elements before there is a gap to the next line of
	/// elements.
	pub width: W,
	/// Total size of a single line of elements (= width + padding).
	pub stride: S,

	pub lifetime: PhantomData<&'a T>
}

impl<'a, T, Tp: AsRef<[T]> + 'a, W: Dimension, S: Dimension>
MatrixBlockStorage<'a, T, Tp, W, S> {
	/// Gets the index into the backing slice given an index relative to the outer rows/cols size 
	fn inner_index(&self, idx: usize) -> usize {
		(idx / self.width.value())*self.stride.value() + (idx % self.width.value())
	}
}

impl<'a, T, Tp: AsRef<[T]> + 'a, W: Dimension, S: Dimension>
StorageType<T> for MatrixBlockStorage<'a, T, Tp, W, S> {
	fn as_ptr(&self) -> *const T { self.data.as_ref().as_ptr() }

	fn alloc(size: usize) -> Self {
		panic!("Can not allocate matrix blocks");
	}
}

impl<'a, T, Tp: AsRef<[T]> + 'a, W: Dimension, S: Dimension>
Index<usize> for MatrixBlockStorage<'a, T, Tp, W, S> {
	type Output = T;
	fn index(&self, idx: usize) -> &T {
		&self.data.as_ref()[self.inner_index(idx)]
	}
}

impl<'a, T, Tp: AsRef<[T]> + AsMut<[T]> + 'a, W: Dimension, S: Dimension>
IndexMut<usize> for MatrixBlockStorage<'a, T, Tp, W, S> {
	fn index_mut(&mut self, idx: usize) -> &mut T {
		let i = self.inner_index(idx);
		&mut self.data.as_mut()[i]
	}
}


/// Helper for looking up the best storage type for representing a matrix based
/// on the specified shape.
pub trait NewStorage<T> {
	// All new storage types will store elements contiguously so we can always
	// take contiguous slices of their contents.
	type Type: StorageTypeMut<T> + AsMut<[T]> + AsRef<[T]> + Clone;
}

impl<T: ElementType> NewStorage<T> for (Dynamic, Dynamic) {
	type Type = Vec<T>;
}
impl<T: ElementType, C: StaticDim> NewStorage<T> for (Dynamic, C) {
	type Type = Vec<T>;
}
impl<T: ElementType, R: StaticDim> NewStorage<T> for (R, Dynamic) {
	type Type = Vec<T>;
}
impl<T: ElementType, R: StaticDim + Mul<C>, C: StaticDim> NewStorage<T> for (R, C)
where <R as Mul<C>>::Output: ArrayLength<T> + Clone {
	type Type = MatrixInlineStorage<T, Prod<R, C>>;
}

// impl<T: Default, R: Dimension, C: Dimension> NewStorage<T> for (R, C)
// where (C, R): NewStorage<T> {
// 	type Type = <(C, R) as NewStorage<T>>::Type;
// } 


