use typenum::{Unsigned, U1, U2, U3, U4, U5, Prod};
use std::marker::PhantomData;
use num_traits::real::Real;
use num_traits::{One, Zero};
use std::ops::{Add, AddAssign, Sub, SubAssign, Mul, MulAssign, Div, DivAssign};
use generic_array::{GenericArray, ArrayLength};
use crate::matrix::dimension::*;
use crate::matrix::storage::*;
use crate::matrix::element::*;

/*
	TODO: Needed operations:
	- AddAssign/SubAssign
	- Mul by a scalar
	- Eigenvector decomposition

	-

*/


#[repr(packed)]
pub struct MatrixBase<T: ElementType, R: Dimension, C: Dimension,
					  D: StorageType<T>> {
	data: D,
	// NOTE: Placed after the data to ensure that the data is aligned if needed.
	rows: R,
	cols: C,

	t: PhantomData<T>, r: PhantomData<R>, c: PhantomData<C>
}


pub type MatrixBlock<'a, T, R, C, S> =
	MatrixBase<T, R, C, MatrixBlockStorage<'a, T, &'a [T], C, S>>;
pub type MatrixBlockMut<'a, T, R, C, S> =
	MatrixBase<T, R, C, MatrixBlockStorage<'a, T, &'a mut [T], C, S>>;

pub type MatrixStatic<T, R: Mul<C>, C> =
	MatrixBase<T, R, C, MatrixInlineStorage<T, Prod<R, C>>>;
pub type VectorStatic<T, R> =
	MatrixBase<T, R, U1, MatrixInlineStorage<T, R>>;

pub type VectorBase<T, R, D> = MatrixBase<T, R, U1, D>;

pub type Matrix<T, R, C> = MatrixBase<T, R, C, Vec<T>>;
pub type Matrix2i = MatrixStatic<isize, U2, U2>;
pub type Matrix3f = MatrixStatic<f32, U3, U3>;
pub type Matrix3d = MatrixStatic<f64, U3, U3>;
pub type Matrix4f = MatrixStatic<f32, U4, U4>;
pub type Matrix4d = MatrixStatic<f64, U4, U4>;
pub type MatrixXd = Matrix<f64, Dynamic, Dynamic>;
pub type Vector<T, R> = Matrix<T, R, U1>;
pub type Vector2i = VectorStatic<isize, U2>;
pub type Vector3f = VectorStatic<f32, U3>;
pub type Vector4f = VectorStatic<f32, U4>;
pub type Vector4d = VectorStatic<f64, U4>;

/// Special alias for selecting the best storage for the given matrix shape.
pub type MatrixNew<T, R, C> =
	MatrixBase<T, R, C, <(R, C) as NewStorage<T>>::Type>;

// TODO: U1 should always be a trivial case.
pub type VectorNew<T, R> =
	MatrixBase<T, R, U1, <(R, U1) as NewStorage<T>>::Type>;


impl<T: ElementType, R: Dimension, C: Dimension, Data: StorageTypeMut<T>>
MatrixBase<T, R, C, Data> {
	// Creates an empty matrix with a dynamic size.
	fn new_with_shape(rows: usize, cols: usize) -> Self {
		// Any static dimensions must match the given dimension.
		if let Some(r) = R::dim() { assert_eq!(rows, r); }
		if let Some(c) = C::dim() { assert_eq!(cols, c); }

		let data = Data::alloc(rows*cols);
		Self {
			data, rows: R::from_usize(rows), cols: C::from_usize(cols),
			r: PhantomData, c: PhantomData, t: PhantomData
		}
	}

	pub fn zero_with_shape(rows: usize, cols: usize) -> Self {
		Self::new_with_shape(rows, cols)
	}

	// TODO: For static dimensions, we need them to match?
	pub fn copy_from<Data2: StorageType<T>>(
		&mut self, other: &MatrixBase<T, R, C, Data2>) {
		assert_eq!(self.rows(), other.rows());
		assert_eq!(self.cols(), other.cols());
		for i in 0..(self.rows()*self.cols()) {
			self.data[i] = other.data[i];
		}
	}

	pub fn copy_from_slice(&mut self, other: &[T]) {
		assert_eq!(self.rows()*self.cols(), other.len());
		for i in 0..other.len() {
			self.data[i] = other[i];
		}
	}
}

// Matrix3d::zero().inverse()

impl<T: ElementType, R: StaticDim, C: StaticDim, D: StorageTypeMut<T>>
MatrixBase<T, R, C, D> {
	// Creates an empty matrix with a statically defined size.
	fn new() -> Self {
		Self::new_with_shape(R::dim().unwrap(), C::dim().unwrap())
	}

	pub fn zero() -> Self {
		Self::new()
	}
}

impl<T: ElementType + Real, R: Dimension, C: Dimension, D: StorageTypeMut<T>>
MatrixBase<T, R, C, D> {
	pub fn identity_with_shape(rows: usize, cols: usize) -> Self {
		let mut m = Self::zero_with_shape(rows, cols);
		for i in 0..rows.min(cols) {
			m[(i, i)] = T::one();
		}
		m
	}
}

impl<T: ElementType + Real, R: StaticDim, C: StaticDim, D: StorageTypeMut<T>>
MatrixBase<T, R, C, D> {
	pub fn identity() -> Self {
		Self::identity_with_shape(R::dim().unwrap(), C::dim().unwrap())
	}
}


impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T> + AsRef<[T]>>
MatrixBase<T, R, C, D> {
	pub fn block_with_shape<RB: Dimension, CB: Dimension>(
		&self, row: usize, col: usize, row_height: usize, col_width: usize)
		-> MatrixBlock<T, RB, CB, C> {
		if let Some(r) = RB::dim() { assert_eq!(row_height, r); }
		if let Some(c) = CB::dim() { assert_eq!(col_width, c); }
		assert!(row + row_height <= self.rows());
		assert!(col + col_width <= self.cols());
		assert!(row_height > 0);
		let start = row*self.cols() + col;
		let end = start + ((row_height - 1)*self.cols() + col_width);
		MatrixBase {
			// XXX: Here we may want to either
			data: MatrixBlockStorage {
				data: &self.data.as_ref()[start..end],
				width: CB::from_usize(col_width),
				stride: self.cols,
				lifetime: PhantomData
			},
			rows: RB::from_usize(row_height),
			cols: CB::from_usize(col_width),
			r: PhantomData, c: PhantomData, t: PhantomData
		}
	}

	pub fn block<RB: StaticDim, CB: StaticDim>(
		&self, row: usize, col: usize) -> MatrixBlock<T, RB, CB, C> {
		self.block_with_shape(row, col, RB::dim().unwrap(), CB::dim().unwrap())
	}

	pub fn col(&self, col: usize) -> MatrixBlock<T, R, U1, C> {
		self.block_with_shape(0, col, self.rows(), 1)
	}

	pub fn row(&self, row: usize) -> MatrixBlock<T, U1, C, C> {
		self.block_with_shape(row, 0, 1, self.cols())
	}
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T> +
	 AsRef<[T]> + AsMut<[T]>>
MatrixBase<T, R, C, D> {
	// TODO: Dedup with above
	pub fn block_with_shape_mut<RB: Dimension, CB: Dimension>(
		&mut self, row: usize, col: usize, row_height: usize, col_width: usize)
		-> MatrixBlockMut<T, RB, CB, C> {

		if let Some(r) = RB::dim() { assert_eq!(row_height, r); }
		if let Some(c) = CB::dim() { assert_eq!(col_width, c); }
		assert!(row + row_height <= self.rows());
		assert!(col + col_width <= self.cols());
		assert!(row_height > 0);
		let start = row*self.cols() + col;
		let end = start + ((row_height - 1)*self.cols() + col_width);
		MatrixBase {
			// XXX: Here we may want to either
			data: MatrixBlockStorage {
				data: &mut self.data.as_mut()[start..end],
				width: CB::from_usize(col_width),
				stride: self.cols,
				lifetime: PhantomData
			},
			rows: RB::from_usize(row_height),
			cols: CB::from_usize(col_width),
			r: PhantomData, c: PhantomData, t: PhantomData
		}


//		unsafe {
//			std::mem::transmute(self.block_with_shape::<RB, CB>(
//				row, col, row_height, col_width))
//		}
	}

	pub fn block_mut<RB: StaticDim, CB: StaticDim>(
		&mut self, row: usize, col: usize) -> MatrixBlockMut<T, RB, CB, C> {
		self.block_with_shape_mut(row, col, RB::dim().unwrap(), CB::dim().unwrap())
	}

	pub fn col_mut(&mut self, col: usize) -> MatrixBlockMut<T, R, U1, C> {
		self.block_with_shape_mut(0, col, self.rows(), 1)
	}

	
}

// as_transpose
// transpose_inplace
// transposed()
// transpose() <- 

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T> + AsRef<[T]>>
MatrixBase<T, R, C, D> {
	/// Constructs a new matrix which references the same data as the current
	/// matrix, but operates as if it were transposed.
	// TODO: Currently only supports vectors
	pub fn as_transpose(&self) -> MatrixBlock<T, C, R, C> {
		assert_eq!(self.cols(), 1);

		MatrixBase {
			// XXX: Here we may want to either
			data: MatrixBlockStorage {
				data: self.data.as_ref(),
				// Don't think too hard into this
				width: self.rows,
				stride: self.cols,
				lifetime: PhantomData
			},
			rows: self.cols,
			cols: self.rows,
			r: PhantomData, c: PhantomData, t: PhantomData
		}
	}

	pub fn transpose(&self) -> MatrixNew<T, C, R> where (C, R): NewStorage<T> {
		let mut out = MatrixNew::zero_with_shape(self.cols(), self.rows());
		for i in 0..out.rows() {
			for j in 0..out.cols() {
				out[(i, j)] = self[(j, i)];
			}
		}
		out
	}
}




impl<T: ElementType + ToString, R: Dimension, C: Dimension, D: StorageType<T>>
std::fmt::Debug for MatrixBase<T, R, C, D> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let mut out: String = "".to_string();
		for i in 0..self.rows() {
			for j in 0..self.cols() {
				out += &self.data[i*self.cols() + j].to_string();
				out += "\t";
			}
			out += "\n";
		}

		write!(f, "{}", out)
	}
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T>>
MatrixBase<T, R, C, D> {
	#[inline]
	pub fn cols(&self) -> usize { self.cols.value() }

	#[inline]
	pub fn rows(&self) -> usize { self.rows.value() }

	// TODO: Only implement for vectors with a shape known to be big enough
	pub fn x(&self) -> T { self[0] }
	pub fn y(&self) -> T { self[1] }
	pub fn z(&self) -> T { self[2] }
	pub fn w(&self) -> T { self[3] }

	pub fn to_owned(&self) -> MatrixNew<T, R, C> where (R, C): NewStorage<T> {
		let mut out = MatrixNew::<T, R, C>::zero_with_shape(
			self.rows(), self.cols());
		out.copy_from(self);
		out
	}
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T>>
MatrixBase<T, R, C, D> {
	pub fn from_slice_with_shape(rows: usize, cols: usize, data: &[T]) -> Self {
		let mut mat = Self::new_with_shape(rows, cols);

		// TODO: Make this more efficient.
		assert_eq!(data.len(), mat.rows()*mat.cols());
		for i in 0..data.len() {
			mat.data[i] = data[i];
		}

//		assert_eq!(mat.data.len(), data.len());
//		mat.data.clone_from_slice(data);

		mat
	}
}

impl<T: ElementType, R: StaticDim, C: StaticDim, D: StorageTypeMut<T>>
MatrixBase<T, R, C, D> {
	pub fn from_slice(data: &[T]) -> Self {
		Self::from_slice_with_shape(R::dim().unwrap(), C::dim().unwrap(), data)
	}

	pub fn as_ptr(&self) -> *const T {
		self.data.as_ptr()
	}
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T>>
std::ops::Index<usize> for MatrixBase<T, R, C, D> {
	type Output = T;
	
	#[inline]
	fn index(&self, i: usize) -> &Self::Output {
		&self.data[i]
	}
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T>>
std::ops::Index<(usize, usize)> for MatrixBase<T, R, C, D> {
	type Output = T;

	#[inline]
	fn index(&self, ij: (usize, usize)) -> &Self::Output {
		assert!(ij.1 < self.cols());
		&self.data[ij.0*self.cols() + ij.1]
	}
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T> /* + AsMut<[T]>*/>
std::ops::IndexMut<(usize, usize)> for MatrixBase<T, R, C, D> {
	#[inline]
	fn index_mut(&mut self, ij: (usize, usize)) -> &mut T {
		let cols = self.cols();
		assert!(ij.1 < cols);
		self.data.index_mut(ij.0*cols + ij.1)
	}
}

////////////////////////////////////////////////////////////////////////////////
// Component-wise Addition/Subtraction/Multiplication/Division
////////////////////////////////////////////////////////////////////////////////

pub trait CwiseMul<Rhs> {
	type Output;
	fn cwise_mul(self, rhs: Rhs) -> Self::Output;
}

pub trait CwiseMulAssign<Rhs> {
	fn cwise_mul_assign(&mut self, rhs: Rhs);
}


pub trait CwiseDiv<Rhs> {
	type Output;
	fn cwise_div(self, rhs: Rhs) -> Self::Output;
}

pub trait CwiseDivAssign<Rhs> {
	fn cwise_div_assign(&mut self, rhs: Rhs);
}

// TODO: When either the RHS or LHS is mutable and passed with ownership, we
// should re-use that buffer rather than creating a new buffer. 

macro_rules! cwise_binary_op {
	($OpAssign:ident, $op_assign:ident, $op_assign_inner:ident,
	 $Op:ident, $op:ident, $op_inner:ident, $op_to:ident) => {
		// += &Matrix
		impl<T: ScalarElementType, R: Dimension, C: Dimension,
			 D: StorageTypeMut<T>, D2: StorageType<T>>
		$OpAssign<&MatrixBase<T, R, C, D2>> for MatrixBase<T, R, C, D> {	
			fn $op_assign(&mut self, rhs: &MatrixBase<T, R, C, D2>) {
				assert_eq!(self.rows(), rhs.rows());
				assert_eq!(self.cols(), rhs.cols());
				for i in 0..(self.rows()*self.cols()) {
					self.data[i].$op_assign_inner(rhs.data[i]);
				}
			}
		}

		// += Matrix
		impl<T: ScalarElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T>,
			D2: StorageType<T>>
		$OpAssign<MatrixBase<T, R, C, D2>> for MatrixBase<T, R, C, D> {
			fn $op_assign(&mut self, rhs: MatrixBase<T, R, C, D2>) {
				self.$op_assign(&rhs);
			}
		}

		// += Scalar
		impl<T: ScalarElementType, R: Dimension, C: Dimension,
			 D: StorageTypeMut<T>, V: num_traits::Num + Copy + Into<T>>
		$OpAssign<V> for MatrixBase<T, R, C, D> {
			fn $op_assign(&mut self, rhs: V) {
				for i in 0..(self.rows()*self.cols()) {
					self.data[i].$op_assign_inner(rhs.into());
				}
			}
		}

		// *out = &Matrix + &Matrix
		impl<T: ScalarElementType, R: Dimension, C: Dimension, D: StorageType<T>>
		MatrixBase<T, R, C, D> {
			/// Performs 'out = self + rhs' overriding any old values in 'out'
			#[inline]
			fn $op_to<D2: StorageType<T>, D3: StorageTypeMut<T>>(
				&self, rhs: &MatrixBase<T, R, C, D2>,
				out: &mut MatrixBase<T, R, C, D3>) {
				assert_eq!(self.rows(), rhs.rows());
				assert_eq!(self.cols(), rhs.cols());
				assert_eq!(self.rows(), out.rows());
				assert_eq!(self.cols(), out.cols());

				for i in 0..(self.rows()*self.cols()) {
					out.data[i] = self.data[i].$op_inner(rhs.data[i]);
				}
			}
		}

		// &Matrix + &Matrix
		impl<T: ScalarElementType, R: Dimension, C: Dimension,
			D: StorageType<T>, D2: StorageType<T>>
		$Op<&MatrixBase<T, R, C, D2>> for &MatrixBase<T, R, C, D>
			where (R, C): NewStorage<T> {
			type Output = MatrixNew<T, R, C>;

			#[inline]
			fn $op(self, rhs: &MatrixBase<T, R, C, D2>) -> Self::Output {
				let mut out = Self::Output::zero_with_shape(self.rows(), self.cols());
				self.$op_to(rhs, &mut out);
				out
			}
		}

		// &Matrix + Matrix
		impl<T: ScalarElementType, R: Dimension, C: Dimension,
			D: StorageType<T>, D2: StorageType<T>>
		$Op<MatrixBase<T, R, C, D2>> for &MatrixBase<T, R, C, D>
			where (R, C): NewStorage<T> {
			type Output = MatrixNew<T, R, C>;

			#[inline]
			fn $op(self, rhs: MatrixBase<T, R, C, D2>) -> Self::Output {
				self.$op(&rhs)
			}
		}

		// Matrix + &Matrix
		impl<T: ScalarElementType, R: Dimension, C: Dimension,
			D: StorageType<T>, D2: StorageType<T>>
		$Op<&MatrixBase<T, R, C, D2>> for MatrixBase<T, R, C, D>
			where (R, C): NewStorage<T> {
			type Output = MatrixNew<T, R, C>;

			#[inline]
			fn $op(mut self, rhs: &MatrixBase<T, R, C, D2>) -> Self::Output {
				(&self).$op(rhs)
			}
		}

		// Matrix + Matrix
		impl<T: ScalarElementType, R: Dimension, C: Dimension,
			D: StorageType<T>, D2: StorageType<T>>
		$Op<MatrixBase<T, R, C, D2>> for MatrixBase<T, R, C, D>
			where (R, C): NewStorage<T> {
			type Output = MatrixNew<T, R, C>;

			#[inline]
			fn $op(self, rhs: MatrixBase<T, R, C, D2>) -> Self::Output {
				(&self).$op(&rhs)
			}
		}

		// &Matrix + Scalar
		impl<T: ScalarElementType, R: Dimension, C: Dimension,
			 D: StorageType<T>, V: num_traits::Num + Copy + Into<T>>
		$Op<V> for &MatrixBase<T, R, C, D>
			where (R, C): NewStorage<T> {
			type Output = MatrixNew<T, R, C>;

			#[inline]
			fn $op(self, rhs: V) -> Self::Output {
				let mut out = Self::Output::zero_with_shape(
					self.rows(), self.cols());
				for i in 0..(self.rows()*self.cols()) {
					out.data[i] = self.data[i].$op_inner(rhs.into());
				}

				out
			}
		}

		// Matrix + Scalar
		impl<T: ScalarElementType, R: Dimension, C: Dimension,
			 D: StorageType<T>, V: num_traits::Num + Copy + Into<T>>
		$Op<V> for MatrixBase<T, R, C, D>
			where (R, C): NewStorage<T> {
			type Output = MatrixNew<T, R, C>;

			#[inline]
			fn $op(self, rhs: V) -> Self::Output {
				(&self).$op(rhs)
			}
		}
	}
}

cwise_binary_op!(AddAssign, add_assign, add_assign, Add, add, add, add_to);
cwise_binary_op!(SubAssign, sub_assign, sub_assign, Sub, sub, sub, sub_to);
cwise_binary_op!(CwiseMulAssign, cwise_mul_assign, mul_assign, CwiseMul, cwise_mul, mul, cwise_mul_to);
cwise_binary_op!(CwiseDivAssign, cwise_div_assign, div_assign, CwiseDiv, cwise_div, div, cwise_div_to);

// Matrix *= Scalar
impl<T: ScalarElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T>, V: num_traits::Num + Copy + Into<T>>
MulAssign<V> for MatrixBase<T, R, C, D> {
	#[inline]
	fn mul_assign(&mut self, rhs: V) {
		for i in 0..(self.rows()*self.cols()) {
			self.data[i] *= rhs.into();
		}
	}
}

// Matrix * Scalar
impl<T: ScalarElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T>, V: num_traits::Num + Copy + Into<T>>
Mul<V> for MatrixBase<T, R, C, D> {
	type Output = Self;

	#[inline]
	fn mul(mut self, rhs: V) -> Self::Output {
		self.mul_assign(rhs);
		self
	}
}

////////////////////////////////////////////////////////////////////////////////
// Matrix Multiplication
////////////////////////////////////////////////////////////////////////////////


// &Matrix * &Matrix
impl<T: ScalarElementType,
	 R: Dimension, S: Dimension, S2: Dimension, C: Dimension,
	 D: StorageType<T>, D2: StorageType<T>>
Mul<&MatrixBase<T, S2, C, D2>> for &MatrixBase<T, R, S, D>
	where (R, C): NewStorage<T>, (S, S2): MaybeEqualDims {
	type Output = MatrixNew<T, R, C>;

	#[inline]
	fn mul(self, rhs: &MatrixBase<T, S2, C, D2>) -> Self::Output {
		assert_eq!(self.cols(), rhs.rows());

		let mut out = Self::Output::new_with_shape(self.rows(), rhs.cols());
		for i in 0..self.rows() {
			for j in 0..rhs.cols() {
				for k in 0..self.cols() {
					out[(i, j)] += self[(i, k)] * rhs[(k, j)];
				}
			}
		}

		out
	}

}

// Matrix * &Matrix
impl<T: ScalarElementType,
	 R: Dimension, S: Dimension, S2: Dimension, C: Dimension,
	 D: StorageType<T>, D2: StorageType<T>>
Mul<&MatrixBase<T, S2, C, D2>> for MatrixBase<T, R, S, D>
	where (R, C): NewStorage<T>, (S, S2): MaybeEqualDims {
	type Output = MatrixNew<T, R, C>;

	#[inline]
	fn mul(self, rhs: &MatrixBase<T, S2, C, D2>) -> Self::Output {
		&self * rhs
	}
}

// &Matrix * Matrix
impl<T: ScalarElementType,
	 R: Dimension, S: Dimension, S2: Dimension, C: Dimension,
	 D: StorageType<T>, D2: StorageType<T>>
Mul<MatrixBase<T, S2, C, D2>> for &MatrixBase<T, R, S, D>
	where (R, C): NewStorage<T>, (S, S2): MaybeEqualDims {
	type Output = MatrixNew<T, R, C>;

	#[inline]
	fn mul(self, rhs: MatrixBase<T, S2, C, D2>) -> Self::Output {
		self * &rhs
	}
}

// Matrix * Matrix
impl<T: ScalarElementType,
	 R: Dimension, S: Dimension, S2: Dimension, C: Dimension,
	 D: StorageType<T>, D2: StorageType<T>>
Mul<MatrixBase<T, S2, C, D2>> for MatrixBase<T, R, S, D>
	where (R, C): NewStorage<T>, (S, S2): MaybeEqualDims {
	type Output = MatrixNew<T, R, C>;

	#[inline]
	fn mul(self, rhs: MatrixBase<T, S2, C, D2>) -> Self::Output {
		&self * &rhs
	}
}

////////////////////////////////////////////////////////////////////////////////


fn argmax<T: std::cmp::PartialOrd, I: Iterator<Item=usize>,
	F: Fn(usize) -> T>(arg: I, func: F) -> Option<usize> {
	let mut max = None;
	for i in arg {
		if max.is_none() || func(i) > func(max.unwrap()) {
			max = Some(i)
		} 
	}

	max
}


impl<T: ScalarElementType, D: StorageType<T>> MatrixBase<T, U3, U1, D> {
	/// TODO: Also have an inplace version and a version that assigns into an
	/// existing buffer.
	pub fn cross<D2: StorageType<T>>(&self, rhs: &MatrixBase<T, U3, U1, D2>)
		-> VectorStatic<T, U3> {
		VectorStatic::<T, U3>::from_slice(&[
			self.y()*rhs.z() - self.z()*rhs.y(),
			self.z()*rhs.x() - self.x()*rhs.z(),
			self.x()*rhs.y() - self.y()*rhs.x()
		])
	}
}

impl<T: ScalarElementType, R: Dimension, C: Dimension, D: StorageType<T>>
MatrixBase<T, R, C, D> {

	pub fn norm_squared(&self) -> T {
		let mut out = T::zero();
		for i in 0..(self.rows()*self.cols()) {
			let v = self[i];
			out += v*v;
		}

		out
	}

	pub fn norm(&self) -> T {
		self.norm_squared().sqrt()
	}

	/// Computes the inner product with another matrix.
	///
	/// The dimensions must exactly match. If you want to perform a dot product
	/// between matrices of different shapes, then you should explicitly reshape
	/// them to be the same shape. 
	pub fn dot<R2: Dimension, C2: Dimension, D2: StorageType<T>>(
		&self, rhs: &MatrixBase<T, R2, C2, D2>) -> T
		where (R, R2): MaybeEqualDims, (C, C2): MaybeEqualDims {
		assert_eq!(self.rows(), rhs.rows());
		assert_eq!(self.cols(), rhs.cols());

		let mut out = T::zero();
		for i in 0..self.rows()*self.cols() {
			out += self[i]*rhs[i]; 
		}

		out
	}

	// TODO: Implement determinant


	// TODO: Must optionally return if it doesn't have an inverse
	pub fn inverse(&self) -> MatrixNew<T, R, C>
		where C: MulDims<U2>,
			  (R, C): NewStorage<T>, (R, ProdDims<C, U2>): NewStorage<T> {
		assert_eq!(self.rows(), self.cols());

		// Form matrix [ self, Identity ].
		let mut m = MatrixNew::<T, R, ProdDims<C, U2>>::new_with_shape(
			self.rows(), 2*self.cols());
		m.block_with_shape_mut::<R, C>(0, 0, self.rows(), self.cols()).copy_from(self);
		m.block_with_shape_mut::<R, C>(0, self.cols(), self.rows(), self.cols())
			.copy_from(&MatrixNew::<T, R, C>::identity_with_shape(self.rows(), self.cols()));

		m.gaussian_elimination();

		// Return right half of the matrix.
		// TODO: Support inverting in-place by copying back from the temp matrix
		// above.
		let mut inv = MatrixBase::new_with_shape(self.rows(), self.cols());
		inv.copy_from(&m.block_with_shape(0, self.cols(), self.rows(), self.cols()));
		inv
	}

	pub fn is_normalized(&self) -> bool {
		T::one() - self.norm_squared() < T::error_epsilon()
	}

	pub fn is_square(&self) -> bool {
		self.rows() == self.cols()
	}

	pub fn is_symmetric(&self) -> bool {
		if !self.is_square() {
			// TODO: Can it be symmetric when not square?
			return false;
		}

		for i in 0..self.rows() {
			for j in 0..i {
				if self[(i, j)] != self[(j, i)] {
					return false;
				}
			}
		}

		true
	}

	/*
	pub fn is_zero(&self) -> bool {

	}

	pub fn is_identity(&self) -> bool {

	}

	pub fn is_diagonal(&self) -> bool {
		for i in 0..self.rows() {
			for j in 0..self.cols() {
				if i == j { continue; }
				if self[(i, j)].abs() > T::error_epsilon() {
					return false;
				}
			}
		}

		true
	}

	// TODO: Should be able to make random matrices and random matrics with 
	// symmetry, etc.

	pub fn is_triangular(&self, upper: bool) -> bool {

	}

	pub fn is_bitriangular(&self) -> bool {

	}

	pub fn is_orthogonal(&self) -> bool {
		(self * self.transpose()).is_identity()
	}
	*/

}

impl<T: ScalarElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T>>
MatrixBase<T, R, C, D> {
	pub fn normalize(&mut self) {
		let n = self.norm();
		self.cwise_div_assign(n);
	}

	pub fn swap_rows(&mut self, i1: usize, i2: usize) {
		if i1 == i2 {
			return;
		}

		for j in 0..self.cols() {
			let temp = self[(i1, j)];
			self[(i1, j)] = self[(i2, j)];
			self[(i2, j)] = temp;
		}
	}

	//  h := 1 /* Initialization of the pivot row */
	//  k := 1 /* Initialization of the pivot column */
	//  while h ≤ m and k ≤ n
	//    /* Find the k-th pivot: */
	//    i_max := argmax (i = h ... m, abs(A[i, k]))
	//    if A[i_max, k] = 0
	//      /* No pivot in this column, pass to next column */
	//      k := k+1
	//    else
	//       swap rows(h, i_max)
	//       /* Do for all rows below pivot: */
	//       for i = h + 1 ... m:
	//          f := A[i, k] / A[h, k]
	//          /* Fill with zeros the lower part of pivot column: */
	//          A[i, k]  := 0
	//          /* Do for all remaining elements in current row: */
	//          for j = k + 1 ... n:
	//             A[i, j] := A[i, j] - A[h, j] * f
	//       /* Increase pivot row and column */
	//       h := h+1 
	//       k := k+1
	pub fn gaussian_elimination(&mut self) {
		let mut h = 0; // Current pivot row.
		let mut k = 0; // Current pivot column.

		while h < self.rows() && k < self.cols() {
			// Find row index with highest value in the current column.
			let i_max = argmax(h..self.rows(), |i| self[(i,k)].abs()).unwrap();
			
			// TODO: Must compare approximately to zero
			if self[(i_max, k)] == T::zero() {
				// This column has no pivot.
				k += 1
			} else {
				self.swap_rows(h, i_max);
				
				// Normalize the pivot row.
				let s = T::one() / self[(h,k)];
				for j in h..self.cols() {
					self[(h, j)] *= s;
				}

				// Use (h+1)..self.rows() if you don't need the upper right to be
				// reduced
				for i in 0..self.rows() {
					if i == h {
						continue;
					}

					let f = self[(i, k)] / self[(h, k)];
					self[(i, k)] = T::zero();
					for j in (k+1)..self.cols() {
						self[(i, j)] = self[(i, j)] - f*self[(h, j)];
					}
				}

				h += 1;
				k += 1;
			}
		}
	}

}


#[cfg(test)]
mod tests {
	use super::*;
    #[test]
    fn it_works() {
		// println!("HELLO WORLD");
		// println!("{:?}", MatrixXd::from_slice_with_shape(2, 2, &[1.0, 2.0, 4.0, 5.0]));
    }

	#[test]
	fn inverse() {
		let m = Matrix3d::from_slice(&[
			0.0, 0.2, 0.0,
			0.5, 1.0, 0.0,
			1.0, 0.0, 0.1
		]);

		let mi = m.inverse();

		println!("{:?}", mi);

		println!("{:?}", m * mi);
	}

	#[test]
	fn matrix_sub() {
		let m = Matrix3d::from_slice(&[
			1.0, 4.0, 9.0,
			2.0, 5.0, 8.0,
			3.0, 6.0, 7.0
		]);
		let m2 = Matrix3d::from_slice(&[
			0.0, 4.0, 9.0,
			2.0, 0.0, 10.0,
			1.0, 6.0, 0.0
		]);

		println!("{:?}", m - m2)
	}

	#[test]
	fn matrix_static_size() {
		// Should be 16, 12, 16, 32
		println!("Vector2i: {}", std::mem::size_of::<Vector2i>());
		println!("Vector3f: {}", std::mem::size_of::<Vector3f>());
		println!("Vector4f: {}", std::mem::size_of::<Vector4f>());
		println!("Vector4d: {}", std::mem::size_of::<Vector4d>());

	}
}
