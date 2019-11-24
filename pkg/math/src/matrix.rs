use typenum::{Unsigned, U1, U2, U3, U4, U5, Prod};
use std::marker::PhantomData;
use num_traits::real::Real;
use std::ops::{AddAssign, Mul};
use generic_array::{GenericArray, ArrayLength};

pub trait Dimension : Copy {
	fn dim() -> Option<usize>;
	fn value(&self) -> usize;
	fn from_usize(value: usize) -> Self;
}

pub trait StaticDim = Unsigned + Copy + Default;

impl<T: StaticDim> Dimension for T {
	fn dim() -> Option<usize> { Some(T::to_usize()) }
	fn value(&self) -> usize { T::to_usize() }
	fn from_usize(value: usize) -> Self {
		assert_eq!(value, T::to_usize());
		T::default()
	}
}

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

pub trait AsPtr<T> {
	fn as_ptr(&self) -> *const T;
}

impl<T> AsPtr<T> for Vec<T> {
	fn as_ptr(&self) -> *const T {
		self.as_ptr()
	}
}


// trait Iteratable<T> {
// 	fn iter(&self) -> Iterator<Item=T>;
// }
// impl<T> Iteratable<T> for Vec<T> { fn iter(&self) -> Iterator<Item=T> { self.iter() } }

pub trait ItemType = Copy + Default + num_traits::Zero;
pub trait ContainerType<T>: std::ops::Index<usize, Output=T> {
	fn alloc(rows: usize, cols: usize) -> Self;
	fn as_ptr(&self) -> *const T;
}

pub trait ContainerTypeMut<T> = ContainerType<T> + std::ops::IndexMut<usize>;

impl<T: ItemType> ContainerType<T> for Vec<T> {
	fn alloc(rows: usize, cols: usize) -> Self {
		let mut out = vec![];
		out.resize(rows*cols, T::zero());
		out
	}

	fn as_ptr(&self) -> *const T {
		AsPtr::as_ptr(self)
	}
}

#[derive(Clone)]
#[repr(packed)]
pub struct MatrixBase<T: ItemType, R: Dimension, C: Dimension, D: ContainerType<T>> {
	rows: R,
	cols: C,
	data: D,

	t: PhantomData<T>, r: PhantomData<R>, c: PhantomData<C>
}

/// Matrix container which stores all elements in stack memory for statically
/// allocatable sizes.
#[derive(Clone)]
#[repr(packed)]
pub struct MatrixInlineContainer<T, N: ArrayLength<T>> {
	data: GenericArray<T, N>
}

impl<T, N: ArrayLength<T>>
std::ops::Index<usize> for MatrixInlineContainer<T, N> {
	type Output = T;
	fn index(&self, idx: usize) -> &T {
		&self.data.as_ref()[idx]
	}
}

impl<T, N: ArrayLength<T>>
std::ops::IndexMut<usize> for MatrixInlineContainer<T, N> {
	fn index_mut(&mut self, idx: usize) -> &mut T {
		&mut self.data.as_mut()[idx]
	}
}

impl<T: ItemType, N: ArrayLength<T>>
ContainerType<T> for MatrixInlineContainer<T, N> {
	fn alloc(rows: usize, cols: usize) -> Self {
		assert_eq!(N::to_usize(), rows*cols);
		Self {
			data: GenericArray::default()
		}
	}

	fn as_ptr(&self) -> *const T {
		self.data.as_ptr()
	}
}



/// A container for a Matrix
pub struct MatrixBlockContainer<'a, T, Tp: AsRef<[T]> + 'a> {
	data: Tp,
	// TODO: Realistically these can always just be passed in?
	rows: usize,
	cols: usize,
	row_stride: usize, // Space in item units between rows.
	lifetime: PhantomData<&'a T>
}

impl<'a, T, Tp: AsRef<[T]> + 'a> MatrixBlockContainer<'a, T, Tp> {
	/// Gets the index into the backing slice given an index relative to the outer rows/cols size 
	fn inner_index(&self, idx: usize) -> usize {
		let row = idx / self.cols;
		let col = idx % self.cols;
		row*self.row_stride + col
	}
}

impl<'a, T, Tp: AsRef<[T]> + 'a>
ContainerType<T> for MatrixBlockContainer<'a, T, Tp> {
	fn as_ptr(&self) -> *const T { self.data.as_ref().as_ptr() }

	fn alloc(rows: usize, cols: usize) -> Self {
		panic!("Can not allocate matrix blocks");
	}
}

impl<'a, T, Tp: AsRef<[T]> + 'a>
std::ops::Index<usize> for MatrixBlockContainer<'a, T, Tp> {
	type Output = T;
	fn index(&self, idx: usize) -> &T { &self.data.as_ref()[self.inner_index(idx)] }
}

impl<'a, T, Tp: AsRef<[T]> + AsMut<[T]> + 'a>
std::ops::IndexMut<usize> for MatrixBlockContainer<'a, T, Tp> {
	fn index_mut(&mut self, idx: usize) -> &mut T {
		let i = self.inner_index(idx);
		&mut self.data.as_mut()[i]
	}
}

pub type MatrixBlock<'a, T, R, C> =
	MatrixBase<T, R, C, MatrixBlockContainer<'a, T, &'a [T]>>;
pub type MatrixBlockMut<'a, T, R, C> =
	MatrixBase<T, R, C, MatrixBlockContainer<'a, T, &'a mut [T]>>;

pub type MatrixStatic<T, R: std::ops::Mul<C>, C> =
	MatrixBase<T, R, C, MatrixInlineContainer<T, Prod<R, C>>>;
pub type VectorStatic<T, R> =
	MatrixBase<T, R, U1, MatrixInlineContainer<T, R>>;

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


impl<T: ItemType, R: Dimension, C: Dimension, Data: ContainerTypeMut<T>>
MatrixBase<T, R, C, Data> {
	// Creates an empty matrix with a dynamic size.
	fn new_with_shape(rows: usize, cols: usize) -> Self {
		// Any static dimensions must match the given dimension.
		if let Some(r) = R::dim() { assert_eq!(rows, r); }
		if let Some(c) = C::dim() { assert_eq!(cols, c); }

//		let mut data = Vec::new();
//		data.resize(rows * cols, T::zero());
		let data = Data::alloc(rows, cols);
		Self {
			data, rows: R::from_usize(rows), cols: C::from_usize(cols),
			r: PhantomData, c: PhantomData, t: PhantomData
		}
	}

	pub fn zero_with_shape(rows: usize, cols: usize) -> Self {
		Self::new_with_shape(rows, cols)
	}
}
impl<T: ItemType, R: Dimension, C: Dimension, Data: ContainerTypeMut<T>>
MatrixBase<T, R, C, Data> {

	// TODO: For static dimensions, we need them to match?
	pub fn copy_from<Data2: ContainerType<T>>(&mut self, other: &MatrixBase<T, R, C, Data2>) {
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

impl<T: ItemType, R: StaticDim, C: StaticDim, D: ContainerTypeMut<T>>
MatrixBase<T, R, C, D> {
	// Creates an empty matrix with a statically defined size.
	fn new() -> Self {
		Self::new_with_shape(R::dim().unwrap(), C::dim().unwrap())
	}

	pub fn zero() -> Self {
		Self::new()
	}
}

impl<T: ItemType + num_traits::real::Real, R: Dimension, C: Dimension,
	D: ContainerTypeMut<T>>
MatrixBase<T, R, C, D> {
	pub fn identity_with_shape(rows: usize, cols: usize) -> Self {
		let mut m = Self::zero_with_shape(rows, cols);
		for i in 0..rows.min(cols) {
			m[(i, i)] = T::one();
		}
		m
	}
}

impl<T: ItemType + Real, R: StaticDim, C: StaticDim, D: ContainerTypeMut<T>>
MatrixBase<T, R, C, D> {
	pub fn identity() -> Self {
		Self::identity_with_shape(R::dim().unwrap(), C::dim().unwrap())
	}
}


impl<T: ItemType, R: Dimension, C: Dimension> Matrix<T, R, C> {
	pub fn block_with_shape<RB: Dimension, CB: Dimension>(
		&self, row: usize, col: usize, row_height: usize, col_width: usize)
		-> MatrixBlock<T, RB, CB> {
		if let Some(r) = RB::dim() { assert_eq!(row_height, r); }
		if let Some(c) = CB::dim() { assert_eq!(col_width, c); }
		assert!(row + row_height <= self.rows());
		assert!(col + col_width <= self.cols());
		assert!(row_height > 0);
		let start = row*self.cols() + col;
		let end = start + ((row_height - 1)*self.cols() + col_width);
		MatrixBase {
			// XXX: Here we may want to either 
			data: MatrixBlockContainer {
				data: &self.data[start..end],
				rows: row_height, cols: col_width,
				row_stride: self.cols(), lifetime: PhantomData
			},
			rows: RB::from_usize(row_height),
			cols: CB::from_usize(col_width),
			r: PhantomData, c: PhantomData, t: PhantomData
		}
	}

	// TODO: Dedup with above
	pub fn block_with_shape_mut<RB: Dimension, CB: Dimension>(
		&mut self, row: usize, col: usize, row_height: usize, col_width: usize)
		-> MatrixBlockMut<T, RB, CB> {

		if let Some(r) = RB::dim() { assert_eq!(row_height, r); }
		if let Some(c) = CB::dim() { assert_eq!(col_width, c); }
		assert!(row + row_height <= self.rows());
		assert!(col + col_width <= self.cols());
		assert!(row_height > 0);
		let start = row*self.cols() + col;
		let end = start + ((row_height - 1)*self.cols() + col_width);
		let row_stride = self.cols();
		MatrixBase {
			// XXX: Here we may want to either
			data: MatrixBlockContainer {
				data: &mut self.data[start..end],
				rows: row_height, cols: col_width,
				row_stride, lifetime: PhantomData
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
}

impl<T: ItemType, R: Dimension, C: Dimension> Matrix<T, R, C> {
	pub fn block<RB: StaticDim, CB: StaticDim>(&self, row: usize, col: usize)
		-> MatrixBlock<T, RB, CB> {
		self.block_with_shape(row, col, RB::dim().unwrap(), CB::dim().unwrap())
	}

	pub fn block_mut<RB: StaticDim, CB: StaticDim>(&mut self, row: usize, col: usize)
		-> MatrixBlockMut<T, RB, CB> {
		self.block_with_shape_mut(row, col, RB::dim().unwrap(), CB::dim().unwrap())
	}
}


impl<T: ItemType + ToString, R: Dimension, C: Dimension, D: ContainerTypeMut<T>> std::fmt::Debug for MatrixBase<T, R, C, D> {
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

impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerType<T>>
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
}

impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerTypeMut<T>>
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

impl<T: ItemType, R: StaticDim, C: StaticDim, D: ContainerTypeMut<T>>
MatrixBase<T, R, C, D> {
	pub fn from_slice(data: &[T]) -> Self {
		Self::from_slice_with_shape(R::dim().unwrap(), C::dim().unwrap(), data)
	}

	pub fn as_ptr(&self) -> *const T {
		self.data.as_ptr()
	}
}

impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerType<T>>
std::ops::Index<usize> for MatrixBase<T, R, C, D> {
	type Output = T;
	
	#[inline]
	fn index(&self, i: usize) -> &Self::Output {
		&self.data[i]
	}
}

impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerTypeMut<T>>
std::ops::Index<(usize, usize)> for MatrixBase<T, R, C, D> {
	type Output = T;

	#[inline]
	fn index(&self, ij: (usize, usize)) -> &Self::Output {
		assert!(ij.1 < self.cols());
		&self.data[ij.0*self.cols() + ij.1]
	}
}

impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerTypeMut<T> /* + AsMut<[T]>*/>
std::ops::IndexMut<(usize, usize)> for MatrixBase<T, R, C, D> {
	#[inline]
	fn index_mut(&mut self, ij: (usize, usize)) -> &mut T {
		let cols = self.cols();
		assert!(ij.1 < cols);
//		&mut self.data.as_mut()[ij.0*cols + ij.1]
		self.data.index_mut(ij.0*cols + ij.1)
	}
}

impl<T: ItemType + std::ops::Add<T>, R: Dimension, C: Dimension,
	 D: ContainerTypeMut<T>, D2: ContainerTypeMut<T>>
std::ops::Add<&MatrixBase<T, R, C, D2>> for &MatrixBase<T, R, C, D> {
	type Output = MatrixBase<T, R, C, D>;

	#[inline]
	fn add(self, other: &MatrixBase<T, R, C, D2>) -> Self::Output {
		assert_eq!(self.rows(), other.rows());
		assert_eq!(self.cols(), other.cols());

		let mut out = Self::Output::zero_with_shape(self.rows(), self.cols());

		for i in 0..(self.rows()*self.cols()) {
			out.data[i] = self.data[i] + other.data[i];
		}

		out
	}
}

// General idea:
// - Move back to C++

// Matrix multiplication: Currently only implemented when the inner dimension types exactly match.
// NOTE: Not implemented when one dimensions is dynamic and the other is static.
//default impl<T: ItemType + num_traits::Zero + AddAssign + Mul<Output=T>,
//	 R: Dimension, S: Dimension, C: Dimension, D: ContainerTypeMut<T>,
//	 D2: ContainerTypeMut<T>>
//Mul<&MatrixBase<T, S, C, D2>> for &MatrixBase<T, R, S, D> {
//	type Output = Matrix<T, R, C>;
//
//	#[inline]
//	fn mul(self, rhs: &MatrixBase<T, S, C, D2>) -> Self::Output {
//		// TODO: Have a single common function for performing the mul.
//		assert_eq!(self.cols(), rhs.rows());
//
//		let mut out = Matrix::<T, R, C>::new_with_shape(self.rows(), rhs.cols());
//		for i in 0..self.rows() {
//			for j in 0..rhs.cols() {
//				for k in 0..self.cols() {
//					out[(i, j)] += self[(i, k)] * rhs[(k, j)];
//				}
//			}
//		}
//
//		out
//	}
//
//}

impl<T: ItemType + num_traits::Zero + AddAssign + Mul<Output=T>,
	R: StaticDim + std::ops::Mul<C>, S: StaticDim, C: StaticDim,
	D: ContainerTypeMut<T>, D2: ContainerTypeMut<T>>
Mul<&MatrixBase<T, S, C, D2>> for &MatrixBase<T, R, S, D>
	where <R as std::ops::Mul<C>>::Output: generic_array::ArrayLength<T> {
	type Output = MatrixStatic<T, R, C>;

	#[inline]
	fn mul(self, rhs: &MatrixBase<T, S, C, D2>) -> Self::Output {
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


// Typically 
// Step one is to do gaussian elimination
// 

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

/*


inline vec3 cross(vec3 u, vec3 v){
	return vec3(

	);
}
*/

impl<T: ItemType + std::ops::Mul<Output=T> + std::ops::Sub<Output=T>,
	 D: ContainerType<T>>
MatrixBase<T, U3, U1, D> {
	/// TODO: Also have an inplace version and a version that assigns into an
	/// existing buffer.
	pub fn cross<D2: ContainerType<T>>(&self, rhs: &MatrixBase<T, U3, U1, D2>)
		-> VectorStatic<T, U3> {
		VectorStatic::<T, U3>::from_slice(&[
			self.y()*rhs.z() - self.z()*rhs.y(),
			self.z()*rhs.x() - self.x()*rhs.z(),
			self.x()*rhs.y() - self.y()*rhs.x()
		])
	}
}

impl<T: ItemType + Real + std::ops::Mul<Output=T> + std::ops::Sub<Output=T> + std::ops::DivAssign + std::ops::MulAssign + std::ops::AddAssign,
	 R: Dimension, C: Dimension, D: ContainerTypeMut<T>>
MatrixBase<T, R, C, D> {

	pub fn norm_squared(&self) -> T {
		let mut out = T::zero();
		for i in 0..(self.rows()*self.cols()) {
			out += self[i];
		}

		out
	}

	pub fn norm(&self) -> T {
		self.norm_squared().sqrt()
	}

	pub fn normalize(&mut self) {
		let n = self.norm();
		for i in 0..(self.rows()*self.cols()) {
			self.data[i] /= n;
		}
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
impl<T: ItemType + std::ops::MulAssign<T> + ToString + num_traits::real::Real,
	R: Dimension, C: Dimension, D: ContainerTypeMut<T>>
MatrixBase<T, R, C, D> {
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

	// TODO: When a fixed size, we want to return with fixed shape

	// TODO: Must optionally return if it doesn't have an inverse
	pub fn inverse(&self) -> Matrix<T, R, C> {
		assert_eq!(self.rows(), self.cols());

		// Form matrix [ self, Identity ].
		let mut m = Matrix::<T, R, Dynamic>::new_with_shape(self.rows(), 2*self.cols());
		m.block_with_shape_mut::<R, C>(0, 0, self.rows(), self.cols()).copy_from(self);
		m.block_with_shape_mut::<R, C>(0, self.cols(), self.rows(), self.cols())
			.copy_from(&Matrix::identity_with_shape(self.rows(), self.cols()));

		m.gaussian_elimination();

		println!("{:?}", m);

		// Return right half of the matrix.
		let mut inv = Matrix::new_with_shape(self.rows(), self.cols());
		inv.copy_from(&m.block_with_shape(0, self.cols(), self.rows(), self.cols()));
		inv
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

		println!("{:?}", &m * &mi);
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

