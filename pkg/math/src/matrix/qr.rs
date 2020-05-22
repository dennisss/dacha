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
pub struct QR<T: ScalarElementType, M: Dimension, N: Dimension>
	where (M, M): NewStorage<T>, (M, N): NewStorage<T> {
	pub q: MatrixNew<T, M, M>,
	pub r: MatrixNew<T, M, N>
}

impl<T: ScalarElementType + ToString + From<f32> + From<u32> + From<i32>, M: Dimension, N: Dimension>
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
			r = &h_full * r;
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

		let expected_q = Matrix3d::from_slice(&[
			-0.857143, 0.394286, 0.331429,
			-0.428571,  -0.902857, -0.034286,
			0.285714, -0.171429, 0.942857
		]).cwise_mul(-1.0);

		let expected_r = Matrix3d::from_slice(&[
			-14.0, -21.0,  14.0,
			0.0, -175.0, 70.0,
			0.0, 0.0, -35.0
		]).cwise_mul(-1.0);

		assert_abs_diff_eq!(qr.q, expected_q, epsilon = 1e-5);
		assert_abs_diff_eq!(qr.r, expected_r, epsilon = 1e-5);
	}

	#[test]
	fn qr_householder() {
		// TODO: Dedup values with above test case

		let m = Matrix3d::from_slice(&[
			12.0, -51.0, 4.0,
			6.0, 167.0, -68.0,
			-4.0, 24.0, -41.0
		]);

		let qr = QR::householder(&m);

		let expected_q = Matrix3d::from_slice(&[
			-0.857143, 0.394286, 0.331429,
			-0.428571,  -0.902857, -0.034286,
			0.285714, -0.171429, 0.942857
		]).cwise_mul(1.0);

		let expected_r = Matrix3d::from_slice(&[
			-14.0, -21.0,  14.0,
			0.0, -175.0, 70.0,
			0.0, 0.0, -35.0
		]).cwise_mul(1.0);

		assert_abs_diff_eq!(qr.q, expected_q, epsilon = 1e-5);
		assert_abs_diff_eq!(qr.r, expected_r, epsilon = 1e-5);
	}

	#[test]
	fn qr_householder2() {
		let m = MatrixXd::from_slice_with_shape(4, 4, &[
			2.0, 0.0, 0.0, 0.0,
			0.0, 1.0, 0.0, 0.0,
			1e-5, 0.0, 1.0, 0.0,
			0.0, 0.0, 0.0, 1.0

//			52.0, 30.0, 49.0, 28.0,
//			30.0, 50.0, 8.0, 44.0,
//			49.0, 8.0, 46.0, 16.0,
//			28.0, 44.0, 16.0, 22.0
		]);

		let qr = QR::householder(&m);

		println!("QQ: {:?}", qr.q);
		println!("RR: {:?}", qr.r);


	}

	/*
	m =

	   52   30   49   28
	   30   50    8   44
	   49    8   46   16
	   28   44   16   22

	octave:2> [q, r] = qr(m)
	q =

	  -0.63110   0.12621   0.56665  -0.51448
	  -0.36410  -0.62926  -0.58771  -0.35504
	  -0.59469   0.55421  -0.44903   0.37089
	  -0.33982  -0.53005   0.36315   0.68680

	r =

	  -82.39539  -56.84784  -66.62994  -50.68245
		0.00000  -46.56525   18.16308  -26.94739
		0.00000    0.00000    8.21908   -9.18850
		0.00000    0.00000    0.00000   -8.98326

	*/

}
