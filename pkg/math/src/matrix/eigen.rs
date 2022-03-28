use core::convert::From;

use num_traits::real::Real;
use typenum::U1;

use crate::matrix::base::*;
use crate::matrix::dimension::*;
use crate::matrix::element::*;
use crate::matrix::householder::*;
use crate::matrix::qr::*;
use crate::matrix::storage::*;

// For a single row, sum up all non-diagonal entry absolute values
// - this will be the radius of the disc centered at the diagonal entry for
//   determining the error bound.

/// TODO: Make these fields private
/// TODO: Allow one dimension to be dynamic as long as the dimensions are
/// MaybeEqual.
#[derive(Debug)]
pub struct EigenStructure<T: ScalarElementType, N: Dimension>
where
    MatrixNewStorage: NewStorage<T, N, N> + NewStorage<T, N, U1>,
{
    /// Each eigen vector will be a column vector of this matrix.
    pub vectors: MatrixNew<T, N, N>,

    /// Each eigen value will be on the diagonal of this matrix. Any non-zero
    /// off-diagonal entries in this will define the confidence of our estimate
    /// of the values
    pub values: MatrixNew<T, N, N>,
}

impl<T: ScalarElementType + From<u32> + From<f32> + From<i32>, N: Dimension> EigenStructure<T, N>
where
    MatrixNewStorage: NewStorage<T, N, N> + NewStorage<T, N, U1> + NewStorage<T, U1, N>,
{
    /// Computes the eigenvalues/vectors using iterative QR decomposition.
    /// See: https://en.wikipedia.org/wiki/QR_algorithm
    pub fn qr_solve<D: StorageType<T, N, N> + AsRef<[T]>>(a: &MatrixBase<T, N, N, D>) -> Self
    where
        MatrixNewStorage: NewStorage<usize, N, U1>,
    {
        assert_eq!(
            a.rows(),
            a.cols(),
            "Decomposition only valid for square matrices"
        );
        let mut a_i = a.to_owned();
        let mut u_i = MatrixNew::<T, N, N>::identity_with_shape(a.rows(), a.cols());

        #[cfg(feature = "std")]
        println!("INPUT MATRIX: {:?}", a);

        // TODO: Check for error bound convergence.
        for i in 0..30 {
            let qr = QR::householder(&a_i);

            a_i = qr.r * &qr.q;
            u_i = u_i * qr.q;

            let max_radius = {
                let radius = gershgorin_radius(&a_i);

                let mut max = T::zero();
                for j in 0..radius.rows() {
                    let v = radius[(j, 0)];
                    if v > max {
                        max = v;
                    }
                }

                max
            };

            if max_radius < (1e-14).into() {
                #[cfg(feature = "std")]
                println!("Eigenvalues converged early after: {} iterations", i + 1);
                break;
            }
        }

        // Sort the eigenvalues in descending order.
        {
            let mut indices = VectorNew::<usize, N>::zero_with_shape(a.rows(), 1);
            for i in 0..a.rows() {
                indices[i] = i;
            }

            // Sort indices by eigenvalue.
            indices
                .as_mut()
                .sort_by(|a, b| a_i[(*b, *b)].partial_cmp(&a_i[(*a, *a)]).unwrap());

            // Indices now contains at the i'th position, the index of the
            // source column that should be the new i'th column

            // Invert the indices.
            let mut indices_inv = VectorNew::<usize, N>::zero_with_shape(a.rows(), 1);
            for i in 0..a.rows() {
                indices_inv[indices[i]] = i;
            }

            // In-place sorting by a permutation in O(n) time.
            // TODO: Generalize this algorithm given any swapping function and
            // an input permutation.
            for i in 0..a.rows() {
                loop {
                    let j = indices_inv[i];
                    if i == j {
                        break;
                    }

                    u_i.swap_cols(i, j);

                    {
                        let tmp = a_i[(i, i)];
                        a_i[(i, i)] = a_i[(j, j)];
                        a_i[(j, j)] = tmp;
                    }

                    {
                        indices_inv[i] = indices_inv[j];
                        indices_inv[j] = j;
                    }
                }
            }
        }

        // TODO: Sort eigenvalues in descending order. (also sorting the cols)

        #[cfg(feature = "std")]
        println!("{:?}", u_i);

        Self {
            values: a_i,
            vectors: u_i,
        }
    }
}

/// Computes the radius for each Gershgorin disk in a matrix.
/// All eigenvalues will fall in a disk within this radius centered at the
/// corresponding diagonal entry of the matrix.
///
/// See https://en.wikipedia.org/wiki/Gershgorin_circle_theorem
pub fn gershgorin_radius<T: ScalarElementType, N: Dimension, S: StorageType<T, N, N>>(
    m: &MatrixBase<T, N, N, S>,
) -> MatrixNew<T, N, U1>
where
    MatrixNewStorage: NewStorage<T, N, U1>,
{
    let mut r = MatrixNew::<T, N, U1>::zero_with_shape(m.rows(), 1);
    for i in 0..m.rows() {
        for j in 0..m.cols() {
            if i != j {
                r[(i, 0)] += m[(i, j)].abs();
            }
        }
    }

    r
}

// Find all disks

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eigen_diag() {
        // This test requires explicitly sorting the eigenvalues.

        let m = MatrixXd::diag(&[5.0, 9.0, 0.0, 4.0]);
        let es = EigenStructure::qr_solve(&m);

        let expected_vals = MatrixXd::diag(&[9.0, 5.0, 4.0, 0.0]);
        let expected_vecs = MatrixXd::from_slice_with_shape(
            4,
            4,
            &[
                0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0,
            ],
        );

        assert_abs_diff_eq!(es.values, expected_vals);
        assert_abs_diff_eq!(es.vectors, expected_vecs);
    }

    #[test]
    fn eigen_qr_method() {
        //		let m = MatrixXd::from_slice_with_shape(6, 6, &[
        //			1.63093, -0.32745, 1.49487, 1.04088, 0.34342, -0.66189,
        //			-0.32745, 0.87722, -0.24105, -0.27757, -0.35714,  0.55214,
        //			1.49487, -0.24105, 2.86802, 2.02600, 0.75627, -0.22412,
        //			1.04088, -0.27757, 2.02600, 2.10248, 0.19245, 0.13652,
        //			0.34342, -0.35714, 0.75627, 0.19245, 2.35152, -1.38161,
        //			-0.66189, 0.55214, -0.22412, 0.13652, -1.38161, 1.31081
        //		]);

        //		let m = MatrixXd::from_slice_with_shape(6, 6, &[
        //			0.68, -0.33, -0.27, -0.717, -0.687, 0.0259,
        //			-0.211, 0.536, 0.0268, 0.214, -0.198, 0.678,
        //			0.566, -0.444, 0.904, -0.967, -0.74, 0.225,
        //			0.597, 0.108, 0.832, -0.514, -0.782, -0.408,
        //			0.823, -0.0452, 0.271, -0.726, 0.998, 0.275,
        //			-0.605, 0.258, 0.435, 0.608, -0.563, 0.0486
        //		]);

        /*
          v =

         0.4369620  -0.2004276   0.4616692   0.6132929   0.0292220  -0.4228148
        -0.2311512  -0.3416734   0.7668991  -0.4421967   0.1704160   0.1308233
        -0.1980033   0.6337183   0.1518957  -0.2301231   0.2259465  -0.6573546
        -0.1408903  -0.6425443  -0.4135432  -0.1569029   0.3549841  -0.4956173
         0.4057185  -0.1152824  -0.0082584  -0.4951764  -0.6970854  -0.3015071
         0.7293630   0.1238386  -0.0675934  -0.3248986   0.5541759   0.1882943

          l =

          Diagonal Matrix

             0.0052169           0           0           0           0           0
                     0   0.2896077           0           0           0           0
                     0           0   0.7372114           0           0           0
                     0           0           0   1.1131834           0           0
                     0           0           0           0   3.1796663           0
                     0           0           0           0           0   5.8160943


          */

        let m = MatrixXd::from_slice_with_shape(
            4,
            4,
            &[
                52.0, 30.0, 49.0, 28.0, 30.0, 50.0, 8.0, 44.0, 49.0, 8.0, 46.0, 16.0, 28.0, 44.0,
                16.0, 22.0,
            ],
        );

        /*
                Eigen values:
                > round(X,5)
                 [,1]    [,2]      [,3]     [,4]
        [1,] 132.6279  0.0000   0.00000  0.00000
        [2,]   0.0000 52.4423   0.00000  0.00000
        [3,]   0.0000  0.0000 -11.54113  0.00000
        [4,]   0.0000  0.0000   0.00000 -3.52904

                Eigen vectors:
                > round(pQ,5)
                        [,1]     [,2]     [,3]     [,4]
                [1,] 0.60946 -0.29992 -0.09988 -0.72707
                [2,] 0.48785  0.65200  0.57725  0.06069
                [3,] 0.46658 -0.60196  0.22156  0.60898
                [4,] 0.41577  0.35013 -0.77956  0.31117


                */

        let es = EigenStructure::qr_solve(&m);

        //		println!("{:?}", es);
    }
}

/*
1.63093, -0.32745, 1.49487, 1.04088, 0.34342, -0.66189; -0.32745, 0.87722, -0.24105, -0.27757, -0.35714,  0.55214; 1.49487, -0.24105, 2.86802, 2.02600, 0.75627, -0.22412; 1.04088, -0.27757, 2.02600, 2.10248, 0.19245, 0.13652; 0.34342, -0.35714, 0.75627, 0.19245, 2.35152, -1.38161; -0.66189, 0.55214, -0.22412, 0.13652, -1.38161, 1.31081


52.0, 30.0, 49.0, 28.0; 30.0, 50.0, 8.0, 44.0; 49.0, 8.0, 46.0, 16.0; 28.0, 44.0, 16.0, 22.0

*/
