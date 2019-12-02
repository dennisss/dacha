use std::convert::From;
use typenum::U1;
use crate::matrix::storage::*;
use crate::matrix::dimension::*;
use crate::matrix::base::*;
use crate::matrix::element::*;


/// Given a unit vector orthogonal to a hyperplane, constructs a transformation
/// matrix that maps any point to its reflection over that plane.
/// 
/// Input: Unit vector of size N
/// Output: Matrix of shape N x N
/// 
/// See: https://en.wikipedia.org/wiki/Householder_transformation
/// TODO: Allow any matrix width which could could be of size
pub fn householder_reflect<T: ScalarElementType + From<u32>,
						   N: Dimension, D: StorageType<T>>
	(v: &VectorBase<T, N, D>) -> MatrixNew<T, N, N>
	where (N, N): NewStorage<T>, (N, U1): NewStorage<T>, (U1, N): NewStorage<T> {
	let n = v.rows();
	assert_eq!(v.cols(), 1);

	let mut v = v.to_owned();
	v.normalize();

	let I = MatrixNew::identity_with_shape(n, n);

	// TODO: Transpose should be able to do this without any copies by taking
	// a reference and flipping dims.
	I - ((&v)*v.transpose()).cwise_mul(2)
}