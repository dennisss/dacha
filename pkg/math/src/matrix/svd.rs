use num_traits::real::Real;
use typenum::U1;
use crate::matrix::dimension::{Dimension};
use crate::matrix::storage::NewStorage;
use crate::matrix::base::{MatrixBase, MatrixNew, CwiseMulAssign, CwiseDivAssign};
use crate::matrix::storage::StorageType;
use crate::matrix::eigen::EigenStructure;
use crate::matrix::element::ScalarElementType;
use crate::matrix::dimension::Dynamic;
use matrix::MatrixXd;
use matrix::element::ErrorEpsilon;

pub struct SVD<T: ScalarElementType, M: Dimension, N: Dimension>
	where (M, M): NewStorage<T>, (M, N): NewStorage<T>, (N, N): NewStorage<T> {
	pub u: MatrixNew<T, M, M>,
	pub s: MatrixNew<T, M, N>,
	pub v: MatrixNew<T, N, N>
}

impl<T: ScalarElementType + ToString + From<i32> + From<f32> + From<u32>, M: Dimension, N: Dimension> SVD<T, M, N>
	where (M, M): NewStorage<T>, (M, N): NewStorage<T>, (N, N): NewStorage<T>, (N, U1): NewStorage<usize>, (M, U1): NewStorage<usize> {
	/// Computes the SVD using an eigenvalue solver.
	/// This is not very efficient as it computed A*A', but it works!
	pub fn eigen_svd<S: StorageType<T> + AsRef<[T]>>(
		a: &MatrixBase<T, M, N, S>) -> Self
		where (M, U1): NewStorage<T>, (U1, M): NewStorage<T>,
			  (N, U1): NewStorage<T>, (U1, N): NewStorage<T>,
			  (N, M): NewStorage<T> {

		// Number of singular values.
		let n = std::cmp::min(a.rows(), a.cols());

		let at = a.transpose();

		let left = a*&at;
		let right = &at*a;

		// TODO: Compute thin left/right based on which one is larger https://math.stackexchange.com/questions/2359992/how-to-resolve-the-sign-issue-in-a-svd-problem

		let eig_left = EigenStructure::qr_solve(&left);
		let mut eig_right = EigenStructure::qr_solve(&right);

		let mut s = MatrixNew::<T, M, N>::zero_with_shape(a.rows(), a.cols());

		s.block_with_shape_mut::<Dynamic, Dynamic>(0, 0, n, n).copy_from(
			&eig_left.values.block_with_shape(0, 0, n, n));

		for i in 0..n {
			s[(i, i)] = s[(i, i)].sqrt();
		}

		// Correct the signs of the right side eigenvectors by computing the
		// corresponding vectors using the left vectors.
		// TODO: thin_right would be sufficient in most cases to avoid computing
		// eig_right.
		let mut thin_right = &at * &eig_left.vectors;
		for i in 0..n {
			thin_right.col_mut(i).cwise_div_assign(s[(i, i)]);

			if (thin_right.col(i) - eig_right.vectors.col(i)).norm()
				> eig_right.vectors.col(i).norm() {
				eig_right.vectors.col_mut(i).cwise_mul_assign(-1);
			}
		}

		Self {
			u: eig_left.vectors,
			s,
			v: eig_right.vectors
		}
	}
}

fn ordered_subspace_eq(a: &MatrixXd, b: &MatrixXd) -> bool {
	assert_eq!(a.rows(), b.rows());
	assert_eq!(a.cols(), b.cols());

	for j in 0..a.cols() {
		let mut val = None;

		for i in 0..a.rows() {
			let a_ij = a[(i, j)];
			let b_ij = b[(i, j)];

			if a_ij.approx_zero() || b_ij.approx_zero() {
				if !(a_ij.approx_zero() && b_ij.approx_zero()) {
					return false;
				}
			} else {
				let scale = a_ij / b_ij;
				if let Some(v) = val {
					if !abs_diff_eq!(scale, v, epsilon = 1e-4) {
						return false;
					}
				} else {
					val = Some(scale);
				}
			}
		}
	}

	true
}

#[cfg(test)]
mod tests {
	use super::*;
	use matrix::{MatrixXd, Matrix};

	#[test]
	fn svd() {
		let m = MatrixXd::from_slice_with_shape(4, 5, &[
			1.0, 0.0, 0.0, 0.0, 2.0,
			0.0, 0.0, 3.0, 0.0, 0.0,
			0.0, 0.0, 0.0, 0.0, 0.0,
			0.0, 2.0, 0.0, 0.0, 0.0
		]);

		let mut svd = SVD::eigen_svd(&m);

		// TODO: There are multiple valid answers for this. The main requirement
		// is that the columns are orthogonal. The final column can be scaled in
		// any way.
		let expected_u = MatrixXd::from_slice_with_shape(4, 4, &[
			0.0, 1.0, 0.0, 0.0,
			1.0, 0.0, 0.0, 0.0,
			0.0, 0.0, 0.0, 1.0,
			0.0, 0.0, 1.0, 0.0
		]);

		assert!(ordered_subspace_eq(&svd.u, &expected_u));

		let expected_s = MatrixXd::diag_with_shape(4, 5,
												   &[3.0, 2.23607, 2.0, 0.0]);
		assert!(ordered_subspace_eq(&svd.s, &expected_s));

		let expected_v = MatrixXd::from_slice_with_shape(5, 5, &[
			0.0, 0.44721, 0.0, 0.0, -0.89443,
			0.0, 0.0, 1.0, 0.0, 0.00000,
			1.0, 0.0, 0.0, 0.0, 0.00000,
			0.0, 0.0, 0.0, 1.0, 0.0,
			0.0, 0.89443, 0.0, 0.0, 0.44721
		]);
		assert!(ordered_subspace_eq(&svd.v, &expected_v));

		// This tests that we have correctly set the signs of the vectors.
		let m_new = &svd.u * &svd.s * svd.v.transpose();
		assert_abs_diff_eq!(m, m_new, epsilon = 1e-9);
	}

	/*
	octave:7> hilb(3)
	ans =

	   1.00000   0.50000   0.33333
	   0.50000   0.33333   0.25000
	   0.33333   0.25000   0.20000

	u =

		-0.82704   0.54745   0.12766
		-0.45986  -0.52829  -0.71375
		-0.32330  -0.64901   0.68867

	  s =

		1.40832  0.00000  0.00000
		0.00000  0.12233  0.00000
		0.00000  0.00000  0.00269

	  v =

		-0.82704   0.54745   0.12766
		-0.45986  -0.52829  -0.71375
		-0.32330  -0.64901   0.68867
	*/
}
