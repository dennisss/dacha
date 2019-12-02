use std::convert::From;
use typenum::U1;
use crate::matrix::storage::*;
use crate::matrix::dimension::*;
use crate::matrix::base::*;
use crate::matrix::element::*;
use crate::matrix::householder::*;
use crate::matrix::qr::*;

// For a single row, sum up all non-diagonal entry absolute values
// - this will be the radius of the disc centered at the diagonal entry for
//   determining the error bound.


/// TODO: Allow one dimension to be dynamic as long as the dimensions are MaybeEqual.
pub struct EigenStructure<T: ScalarElementType, N: Dimension>
where (N, N): NewStorage<T>, (N, U1): NewStorage<T> {
	/// Each eigen vector will be a column vector of this matrix. 
	vectors: MatrixNew<T, N, N>,

	/// Each eigen value will be on the diagonal of this matrix. Any non-zero
	/// off-diagonal entries in this will define the confidence of our estimate
	/// of the values
	values: MatrixNew<T, N, N>
}

impl<T: ScalarElementType + ToString + From<u32> + From<i32>, N: Dimension>
EigenStructure<T, N>
where (N, N): NewStorage<T>, (N, U1): NewStorage<T>, (U1, N): NewStorage<T> {

	/// Computes the eigenvalues/vectors using iterative QR decomposition.
	/// See: https://en.wikipedia.org/wiki/QR_algorithm
	pub fn qr_solve<D: StorageType<T> + AsRef<[T]>>(
		a: &MatrixBase<T, N, N, D>) -> Self {
		assert_eq!(a.rows(), a.cols(),
				   "Decomposition only valid for square matrices");
		let mut a_i = a.to_owned();
		let mut u_i = MatrixNew::<T, N, N>::identity_with_shape(
			a.rows(), a.cols());
		
		// TODO: Check for error bound convergence.
		for i in 0..100 {
			let qr = QR::householder(&a_i);
			a_i = qr.r * &qr.q;
			u_i = u_i * qr.q;

			println!("{:?}", a_i);
		}

		Self { values: a_i, vectors: u_i }

	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn eigen_qr_method() {
		let m = MatrixXd::from_slice_with_shape(6, 6, &[
			0.68, -0.33, -0.27, -0.717, -0.687, 0.0259,
			-0.211, 0.536, 0.0268, 0.214, -0.198, 0.678,
			0.566, -0.444, 0.904, -0.967, -0.74, 0.225,
			0.597, 0.108, 0.832, -0.514, -0.782, -0.408,
			0.823, -0.0452, 0.271, -0.726, 0.998, 0.275,
			-0.605, 0.258, 0.435, 0.608, -0.563, 0.0486
		]);

		let es = EigenStructure::qr_solve(&m);
	}
}