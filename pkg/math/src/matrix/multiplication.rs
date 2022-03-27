// Matrix * Matrix Multiplication
//
// Implements:
// => a.mul_to(&b, &mut out);
// => c = &a * &b;
// => c = a * b;
// => c = &a * b;
// => c = a * &b;

use core::ops::Mul;

use crate::matrix::base::{MatrixBase, MatrixNew};
use crate::matrix::dimension::{Dimension, MaybeEqualDims};
use crate::matrix::element::ScalarElementType;
use crate::matrix::storage::{MatrixNewStorage, NewStorage, StorageType, StorageTypeMut};

impl<T: ScalarElementType, R: Dimension, S: Dimension, D: StorageType<T, R, S>>
    MatrixBase<T, R, S, D>
{
    #[inline]
    pub fn mul_to<
        S2: Dimension,
        C: Dimension,
        D2: StorageType<T, S2, C>,
        D3: StorageTypeMut<T, R, C>,
    >(
        &self,
        rhs: &MatrixBase<T, S2, C, D2>,
        out: &mut MatrixBase<T, R, C, D3>,
    ) where
        (S, S2): MaybeEqualDims,
    {
        assert_eq!(self.cols(), rhs.rows());
        for i in 0..self.rows() {
            for j in 0..rhs.cols() {
                let val = &mut out[(i, j)];
                *val = T::zero();
                for k in 0..self.cols() {
                    *val += self[(i, k)] * rhs[(k, j)];
                }
            }
        }
    }
}

// &Matrix * &Matrix
impl<
        T: ScalarElementType,
        R: Dimension,
        S: Dimension,
        S2: Dimension,
        C: Dimension,
        D: StorageType<T, R, S>,
        D2: StorageType<T, S2, C>,
    > Mul<&MatrixBase<T, S2, C, D2>> for &MatrixBase<T, R, S, D>
where
    MatrixNewStorage: NewStorage<T, R, C>,
    (S, S2): MaybeEqualDims,
{
    type Output = MatrixNew<T, R, C>;

    #[inline]
    fn mul(self, rhs: &MatrixBase<T, S2, C, D2>) -> Self::Output {
        let mut out = Self::Output::new_with_shape(self.rows(), rhs.cols());
        self.mul_to(rhs, &mut out);
        out
    }
}

// Matrix * &Matrix
impl<
        T: ScalarElementType,
        R: Dimension,
        S: Dimension,
        S2: Dimension,
        C: Dimension,
        D: StorageType<T, R, S>,
        D2: StorageType<T, S2, C>,
    > Mul<&MatrixBase<T, S2, C, D2>> for MatrixBase<T, R, S, D>
where
    MatrixNewStorage: NewStorage<T, R, C>,
    (S, S2): MaybeEqualDims,
{
    type Output = MatrixNew<T, R, C>;

    #[inline]
    fn mul(self, rhs: &MatrixBase<T, S2, C, D2>) -> Self::Output {
        &self * rhs
    }
}

// &Matrix * Matrix
impl<
        T: ScalarElementType,
        R: Dimension,
        S: Dimension,
        S2: Dimension,
        C: Dimension,
        D: StorageType<T, R, S>,
        D2: StorageType<T, S2, C>,
    > Mul<MatrixBase<T, S2, C, D2>> for &MatrixBase<T, R, S, D>
where
    MatrixNewStorage: NewStorage<T, R, C>,
    (S, S2): MaybeEqualDims,
{
    type Output = MatrixNew<T, R, C>;

    #[inline]
    fn mul(self, rhs: MatrixBase<T, S2, C, D2>) -> Self::Output {
        self * &rhs
    }
}

// Matrix * Matrix
impl<
        T: ScalarElementType,
        R: Dimension,
        S: Dimension,
        S2: Dimension,
        C: Dimension,
        D: StorageType<T, R, S>,
        D2: StorageType<T, S2, C>,
    > Mul<MatrixBase<T, S2, C, D2>> for MatrixBase<T, R, S, D>
where
    MatrixNewStorage: NewStorage<T, R, C>,
    (S, S2): MaybeEqualDims,
{
    type Output = MatrixNew<T, R, C>;

    #[inline]
    fn mul(self, rhs: MatrixBase<T, S2, C, D2>) -> Self::Output {
        &self * &rhs
    }
}
