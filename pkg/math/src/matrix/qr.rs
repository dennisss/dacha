use std::convert::From;
use typenum::U1;
use crate::matrix::storage::*;
use crate::matrix::dimension::*;
use crate::matrix::base::*;
use crate::matrix::element::*;
use crate::matrix::householder::*;

/// QR Decomposition of a square real matrix into an orthogonal matrix and an
/// upper triangular matrix
/// 
/// NOTE: The decomposition is only valid when M >= N
pub struct QR<T: ScalarElementType, M: Dimension, N: Dimension> where (M, M): NewStorage<T>, (M, N): NewStorage<T> {
	pub q: MatrixNew<T, M, M>,
	pub r: MatrixNew<T, M, N>
}

impl<T: ScalarElementType + ToString + From<u32> + From<i32>, M: Dimension, N: Dimension>
QR<T, M, N>
where (M, M): NewStorage<T>, (M, N): NewStorage<T>, (N, M): NewStorage<T>,
	  (M, U1): NewStorage<T>, (U1, M): NewStorage<T> {

	/// See:
	/// https://en.wikipedia.org/wiki/QR_decomposition#Using_the_Gram%E2%80%93Schmidt_process
	pub fn gram_schmidt<D: StorageType<T> + AsRef<[T]>>(
		a: &MatrixBase<T, M, N, D>) -> Self {
		assert!(a.rows() >= a.cols());

		let mut u = a.to_owned();

		// TODO: Use numerically stable version and normalize as we go:
		// https://en.wikipedia.org/wiki/Gram%E2%80%93Schmidt_process#Numerical_stability
		for i in 1..u.cols() {
			for j in 0..i {
				let p = proj(&u.col(j), &a.col(i));
				u.col_mut(i).sub_assign(p);
			}
		}

		let e = {
			for i in 0..u.cols() {
				u.col_mut(i).normalize();
			}

			u
		};

		// q = e padded with zeros
		// TODO: Test this with a matrix with more rows than columns
		let q = {
			let mut z = MatrixNew::<T, M, M>::identity_with_shape(
				a.rows(), a.rows());
			z.block_with_shape_mut(0, 0, e.rows(), e.cols()).copy_from(
				&e);
			z	
		};


		let r = q.transpose() * a;

		// TODO: Assert r is upper triangular.
		// TODO: Assert q is orthogonal aka. q*q.tranpose() = q

		Self { q, r }
	}

	/// See:
	/// https://en.wikipedia.org/wiki/QR_decomposition#Using_Householder_reflections
	pub fn householder<D: StorageType<T> + AsRef<[T]>>(
		a: &MatrixBase<T, M, N, D>) -> Self {
		
		let t = std::cmp::min(a.rows() - 1, a.cols());

		let mut q = MatrixNew::<T, M, M>::identity_with_shape(
			a.rows(), a.rows());
		let mut r = a.to_owned();

		for i in 0..t {
			let mut col = r.block_with_shape::<Dynamic, U1>(
				i, i, a.rows() - i, 1).to_owned();

			// TODO: Should have the opposite sign as the entry.
			let mut e = col.norm();
			if col[(0,0)] > T::zero() {
				e *= (-1).into()
			}

			col[(0,0)] -= e; 

			let h = householder_reflect::<T, Dynamic, _>(&col);

			// Padded in the upper-left with an identity matrix.
			let mut h_full = MatrixNew::<T, M, M>::identity_with_shape(
				a.rows(), a.rows());
			h_full.block_with_shape_mut(i, i, a.rows() - i, a.rows() - i)
				.copy_from(&h);

			q = q * h_full.transpose();
			r = h_full * r;

			println!("{:?}", r);
		}

		Self { q, r }
	}

}

fn proj<T: ScalarElementType, N: Dimension, D: StorageType<T>,
		D2: StorageType<T>>(
	u: &VectorBase<T, N, D>, a: &VectorBase<T, N, D2>) -> VectorNew<T, N>
	where (N, U1): NewStorage<T> {
	u.cwise_mul(u.dot(a) / u.dot(u))
}


// mxm   mxm

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn qr_gram_schmidt() {
		let m = Matrix3d::from_slice(&[
			12.0, -51.0, 4.0,
			6.0, 167.0, -68.0,
			-4.0, 24.0, -41.0
		]);

		let qr = QR::gram_schmidt(&m);
		println!("Q: {:?}\nR:{:?}", qr.q, qr.r);

		println!("Q*R: {:?}", qr.q * qr.r);
	}

	#[test]
	fn qr_householder() {
		let m = Matrix3d::from_slice(&[
			12.0, -51.0, 4.0,
			6.0, 167.0, -68.0,
			-4.0, 24.0, -41.0
		]);

		let qr = QR::householder(&m);

		// TODO: Should start checking for approximate equivalence.
		println!("Q: {:?}\nR:{:?}", qr.q, qr.r);

		println!("Q*R: {:?}", qr.q * qr.r);
	}
}
