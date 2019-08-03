use typenum::{Unsigned, U1, U2, U3, U4, U5};
use std::marker::PhantomData;

pub trait Dimension {
	fn dim() -> Option<usize>;
}

impl<T: Unsigned> Dimension for T {
	fn dim() -> Option<usize> { Some(T::to_usize()) }
}

pub struct Dynamic;
impl Dimension for Dynamic {
	fn dim() -> Option<usize> { None }
}

// trait Iteratable<T> {
// 	fn iter(&self) -> Iterator<Item=T>;
// }
// impl<T> Iteratable<T> for Vec<T> { fn iter(&self) -> Iterator<Item=T> { self.iter() } }

pub trait ItemType = Copy + num_traits::Zero;
pub trait ContainerType<T> = std::ops::Index<usize, Output=T>;

pub struct MatrixBase<T: ItemType, R: Dimension, C: Dimension, D: ContainerType<T>> {
	rows: usize,
	cols: usize,
	data: D,

	t: PhantomData<T>, r: PhantomData<R>, c: PhantomData<C>
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

impl<'a, T, Tp: AsRef<[T]> + 'a> std::ops::Index<usize> for MatrixBlockContainer<'a, T, Tp> {
	type Output = T;
	fn index(&self, idx: usize) -> &T { &self.data.as_ref()[self.inner_index(idx)] }
}

impl<'a, T, Tp: AsRef<[T]> + AsMut<[T]> + 'a> std::ops::IndexMut<usize> for MatrixBlockContainer<'a, T, Tp> {
	fn index_mut(&mut self, idx: usize) -> &mut T { let i = self.inner_index(idx); &mut self.data.as_mut()[i] }
}

pub type MatrixBlock<'a, T, R, C> = MatrixBase<T, R, C, MatrixBlockContainer<'a, T, &'a [T]>>;
pub type MatrixBlockMut<'a, T, R, C> = MatrixBase<T, R, C, MatrixBlockContainer<'a, T, &'a mut [T]>>;

pub type Matrix<T, R, C> = MatrixBase<T, R, C, Vec<T>>;
pub type Matrix2i = Matrix<usize, U2, U2>;
pub type Matrix3f = Matrix<f32, U3, U3>;
pub type Matrix3d = Matrix<f64, U3, U3>;
pub type Matrix4f = Matrix<f32, U4, U4>;
pub type Matrix4d = Matrix<f64, U4, U4>;
pub type MatrixXd = Matrix<f64, Dynamic, Dynamic>;
pub type Vector<T, R> = Matrix<T, R, U1>;

impl<T: ItemType, R: Dimension, C: Dimension, Data: ContainerType<T>> MatrixBase<T, R, C, Data> {
	// Creates an empty matrix with a dynamic size.
	fn new_with_shape(rows: usize, cols: usize) -> Matrix<T, R, C> {
		// Any static dimensions must match the given dimension.
		if let Some(r) = R::dim() { assert_eq!(rows, r); }
		if let Some(c) = C::dim() { assert_eq!(cols, c); }

		let mut data = Vec::new();
		data.resize(rows * cols, T::zero());
		Matrix { data, rows, cols, r: PhantomData, c: PhantomData, t: PhantomData }
	}

	pub fn zero_with_shape(rows: usize, cols: usize) -> Matrix<T, R, C> {
		Self::new_with_shape(rows, cols)
	}
}
impl<T: ItemType, R: Dimension, C: Dimension, Data: ContainerType<T> + std::ops::IndexMut<usize>> MatrixBase<T, R, C, Data> {

	// TODO: For static dimensions, we need them to match?
	pub fn copy_from<Data2: ContainerType<T>>(&mut self, other: &MatrixBase<T, R, C, Data2>) {
		assert_eq!(self.rows, other.rows);
		assert_eq!(self.cols, other.cols);
		for i in 0..(self.rows*self.cols) {
			self.data[i] = other.data[i];
		}
	}

	pub fn copy_from_slice(&mut self, other: &[T]) {
		assert_eq!(self.rows*self.cols, other.len());
		for i in 0..other.len() {
			self.data[i] = other[i];
		}
	}
}

// Matrix3d::zero().inverse()

impl<T: ItemType, R: Unsigned, C: Unsigned, D: ContainerType<T>> MatrixBase<T, R, C, D> {
	// Creates an empty matrix with a statically defined size.
	fn new() -> Matrix<T, R, C> {
		Self::new_with_shape(R::dim().unwrap(), C::dim().unwrap())
	}

	pub fn zero() -> Matrix<T, R, C> {
		Self::new()
	}
}

impl<T: ItemType + num_traits::real::Real, R: Dimension, C: Dimension, D: ContainerType<T>> MatrixBase<T, R, C, D> {
	pub fn identity_with_shape(rows: usize, cols: usize) -> Matrix<T, R, C> {
		let mut m = Matrix::zero_with_shape(rows, cols);
		for i in 0..rows.min(cols) {
			m[(i, i)] = T::one();
		}
		m
	}
}

impl<T: ItemType + num_traits::real::Real, R: Unsigned, C: Unsigned, D: ContainerType<T>> MatrixBase<T, R, C, D> {
	pub fn identity() -> Matrix<T, R, C> {
		Self::identity_with_shape(R::dim().unwrap(), C::dim().unwrap())
	}
}


impl<T: ItemType, R: Dimension, C: Dimension> Matrix<T, R, C> {
	pub fn block_with_shape<RB: Dimension, CB: Dimension>(&self, row: usize, col: usize, row_height: usize, col_width: usize) -> MatrixBlock<T, RB, CB> {
		if let Some(r) = RB::dim() { assert_eq!(row_height, r); }
		if let Some(c) = CB::dim() { assert_eq!(col_width, c); }
		assert!(row + row_height <= self.rows);
		assert!(col + col_width <= self.cols);
		assert!(row_height > 0);
		let start = row*self.cols + col;
		let end = start + ((row_height - 1)*self.cols + col_width);
		MatrixBase {
			// XXX: Here we may want to either 
			data: MatrixBlockContainer { data: &self.data[start..end], rows: row_height, cols: col_width, row_stride: self.cols, lifetime: PhantomData },
			rows: row_height,
			cols: col_width,
			r: PhantomData, c: PhantomData, t: PhantomData
		}
	}
	
	pub fn block_with_shape_mut<RB: Dimension, CB: Dimension>(&mut self, row: usize, col: usize, row_height: usize, col_width: usize) -> MatrixBlockMut<T, RB, CB> {
		unsafe { std::mem::transmute(self.block_with_shape::<RB, CB>(row, col, row_height, col_width)) }
	}
}

impl<T: ItemType, R: Dimension, C: Dimension> Matrix<T, R, C> {
	pub fn block<RB: Unsigned, CB: Unsigned>(&self, row: usize, col: usize) -> MatrixBlock<T, RB, CB> {
		self.block_with_shape(row, col, RB::dim().unwrap(), CB::dim().unwrap())
	}

	pub fn block_mut<RB: Unsigned, CB: Unsigned>(&mut self, row: usize, col: usize) -> MatrixBlockMut<T, RB, CB> {
		self.block_with_shape_mut(row, col, RB::dim().unwrap(), CB::dim().unwrap())
	}
}


impl<T: ItemType + ToString, R: Dimension, C: Dimension, D: ContainerType<T>> std::fmt::Debug for MatrixBase<T, R, C, D> {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		let mut out: String = "".to_string();
		for i in 0..self.rows {
			for j in 0..self.cols {
				out += &self.data[i*self.cols + j].to_string();
				out += "\t";
			}
			out += "\n";
		}

		write!(f, "{}", out)
	}
}

impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerType<T>> MatrixBase<T, R, C, D> {
	pub fn from_slice_with_shape(rows: usize, cols: usize, data: &[T]) -> Matrix<T, R, C> {
		let mut mat = Matrix::new_with_shape(rows, cols);
		assert_eq!(mat.data.len(), data.len());
		mat.data.clone_from_slice(data);
		mat
	}

	pub fn cols(&self) -> usize {
		self.cols
	}

	pub fn rows(&self) -> usize {
		self.rows
	}
}

impl<T: ItemType, R: Unsigned, C: Unsigned, D: ContainerType<T>> MatrixBase<T, R, C, D> {
	pub fn from_slice(data: &[T]) -> Matrix<T, R, C> {
		Self::from_slice_with_shape(R::dim().unwrap(), C::dim().unwrap(), data)
	}
}


impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerType<T>> std::ops::Index<usize> for MatrixBase<T, R, C, D> {
	type Output = T;
	
	#[inline]
	fn index(&self, i: usize) -> &Self::Output {
		&self.data[i]
	}
}

impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerType<T>> std::ops::Index<(usize, usize)> for MatrixBase<T, R, C, D> {
	type Output = T;

	#[inline]
	fn index(&self, ij: (usize, usize)) -> &Self::Output {
		assert!(ij.1 < self.cols);
		&self.data[ij.0*self.cols + ij.1]
	}
}

impl<T: ItemType, R: Dimension, C: Dimension, D: ContainerType<T> + AsMut<[T]>> std::ops::IndexMut<(usize, usize)> for MatrixBase<T, R, C, D> {
	#[inline]
	fn index_mut(&mut self, ij: (usize, usize)) -> &mut T {
		assert!(ij.1 < self.cols);
		&mut self.data.as_mut()[ij.0*self.cols + ij.1]
	}
}

impl<T: ItemType + std::ops::Add<T>, R: Dimension, C: Dimension,
	 D: ContainerType<T>, D2: ContainerType<T>>
	 std::ops::Add<&MatrixBase<T, R, C, D2>> for &MatrixBase<T, R, C, D> {
	type Output = MatrixBase<T, R, C, Vec<T>>;

	#[inline]
	fn add(self, other: &MatrixBase<T, R, C, D2>) -> Self::Output {
		assert_eq!(self.rows, other.rows);
		assert_eq!(self.cols, other.cols);

		let mut data = Vec::new();

		for i in 0..(self.rows*self.cols) {
			data.push(self.data[i] + other.data[i]);
		}

		MatrixBase {
			rows: self.rows,
			cols: self.cols,
			data,
			r: PhantomData, c: PhantomData, t: PhantomData
		}
	}
}

// General idea:
// - Move back to C++

// Matrix multiplication: Currently only implemented when the inner dimension types exactly match.
// NOTE: Not implemented when one dimensions is dynamic and the other is static.
impl<T: ItemType + num_traits::Zero + std::ops::AddAssign + std::ops::Mul<Output=T>,
	 R: Dimension, S: Dimension, C: Dimension, D: ContainerType<T>>
std::ops::Mul<Matrix<T, S, C>> for MatrixBase<T, R, S, D> {
	type Output = Matrix<T, R, C>;

	#[inline]
	fn mul(self, rhs: Matrix<T, S, C>) -> Self::Output {
		assert_eq!(self.cols, rhs.rows);

		let mut out = Matrix::<T, R, C>::new_with_shape(self.rows, rhs.cols);
		for i in 0..self.rows {
			for j in 0..rhs.cols {
				for k in 0..self.cols {
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

fn argmax<T: std::cmp::PartialOrd, I: Iterator<Item=usize>, F: Fn(usize) -> T>(arg: I, func: F) -> Option<usize> {
	let mut max = None;
	for i in arg {
		if max.is_none() || func(i) > func(max.unwrap()) {
			max = Some(i)
		} 
	}

	max
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
impl<T: ItemType + std::ops::MulAssign<T> + ToString + num_traits::real::Real, R: Dimension, C: Dimension, D: ContainerType<T> + AsMut<[T]>> MatrixBase<T, R, C, D> {
	pub fn swap_rows(&mut self, i1: usize, i2: usize) {
		if i1 == i2 {
			return;
		}

		for j in 0..self.cols {
			let temp = self[(i1, j)];
			self[(i1, j)] = self[(i2, j)];
			self[(i2, j)] = temp;
		}
	}

	pub fn gaussian_elimination(&mut self) {
		let mut h = 0; // Current pivot row.
		let mut k = 0; // Current pivot column.

		while h < self.rows && k < self.cols {
			// Find row index with highest value in the current column.
			let i_max = argmax(h..self.rows, |i| self[(i,k)].abs()).unwrap();
			
			// TODO: Must compare approximately to zero
			if self[(i_max, k)] == T::zero() {
				// This column has no pivot.
				k += 1
			} else {
				self.swap_rows(h, i_max);
				
				// Normalize the pivot row.
				let s = T::one() / self[(h,k)];
				for j in h..self.cols {
					self[(h, j)] *= s;
				}

				for i in 0..self.rows { // Use (h+1)..self.rows if you don't need the upper right to be reduced
					if i == h {
						continue;
					}

					let f = self[(i, k)] / self[(h, k)];
					self[(i, k)] = T::zero();
					for j in (k+1)..self.cols {
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
		assert_eq!(self.rows, self.cols);

		// Form matrix [ self, Identity ].
		let mut m = Matrix::<T, R, Dynamic>::new_with_shape(self.rows, 2*self.cols);
		m.block_with_shape_mut::<R, C>(0, 0, self.rows, self.cols).copy_from(self);
		m.block_with_shape_mut::<R, C>(0, self.cols, self.rows, self.cols)
			.copy_from(&Matrix::identity_with_shape(self.rows, self.cols));

		m.gaussian_elimination();

		println!("{:?}", m);

		// Return right half of the matrix.
		let mut inv = Matrix::new_with_shape(self.rows, self.cols);
		inv.copy_from(&m.block_with_shape(0, self.cols, self.rows, self.cols));
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

		println!("{:?}", m*mi);
	}
}

