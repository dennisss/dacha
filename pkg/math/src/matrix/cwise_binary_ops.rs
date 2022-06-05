// Component-wise Addition/Subtraction/Multiplication/Division

use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use crate::matrix::base::{MatrixBase, MatrixNew};
use crate::matrix::dimension::Dimension;
use crate::matrix::element::ScalarElementType;
use crate::matrix::storage::{MatrixNewStorage, NewStorage, StorageType, StorageTypeMut};

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

pub trait CwiseMin<Rhs> {
    type Output;
    fn cwise_min(self, rhs: Rhs) -> Self::Output;
}

pub trait CwiseMinAssign<Rhs> {
    fn cwise_min_assign(&mut self, rhs: Rhs);
}

pub trait CwiseMax<Rhs> {
    type Output;
    fn cwise_max(self, rhs: Rhs) -> Self::Output;
}

pub trait CwiseMaxAssign<Rhs> {
    fn cwise_max_assign(&mut self, rhs: Rhs);
}

// TODO: When either the RHS or LHS is mutable and passed with ownership, we
// should re-use that buffer rather than creating a new buffer.

// OpAssign        : First trait to implement for MatrixBase.
// op_assign       : Function in OpAssign to implement
// op_assign_inner : Function with signature f(self: &mut T, other: &T) used to
//                   implement op_assign on each element.
// Op              : Second trait to implement for MatrixBase.
// op
// op_inner        : Function with signature f(self: T, other: &T)
macro_rules! cwise_binary_op {
    ($OpAssign:ident, $op_assign:ident, $op_assign_inner:expr,
	 $Op:ident, $op:ident, $op_inner:expr, $op_to:ident) => {
        // += &Matrix
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageTypeMut<T, R, C>,
                D2: StorageType<T, R, C>,
            > $OpAssign<&MatrixBase<T, R, C, D2>> for MatrixBase<T, R, C, D>
        {
            fn $op_assign(&mut self, rhs: &MatrixBase<T, R, C, D2>) {
                assert_eq!(self.rows(), rhs.rows());
                assert_eq!(self.cols(), rhs.cols());
                for i in 0..(self.rows() * self.cols()) {
                    $op_assign_inner(&mut self.data[i], rhs.data[i]);
                }
            }
        }

        // += Matrix
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageTypeMut<T, R, C>,
                D2: StorageType<T, R, C>,
            > $OpAssign<MatrixBase<T, R, C, D2>> for MatrixBase<T, R, C, D>
        {
            fn $op_assign(&mut self, rhs: MatrixBase<T, R, C, D2>) {
                self.$op_assign(&rhs);
            }
        }

        // += Scalar
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageTypeMut<T, R, C>,
                V: num_traits::Num + Copy + Into<T>,
            > $OpAssign<V> for MatrixBase<T, R, C, D>
        {
            fn $op_assign(&mut self, rhs: V) {
                for i in 0..(self.rows() * self.cols()) {
                    $op_assign_inner(&mut self.data[i], rhs.into());
                }
            }
        }

        // *out = &Matrix + &Matrix
        impl<T: ScalarElementType, R: Dimension, C: Dimension, D: StorageType<T, R, C>>
            MatrixBase<T, R, C, D>
        {
            /// Performs 'out = self + rhs' overriding any old values in 'out'
            #[inline]
            fn $op_to<D2: StorageType<T, R, C>, D3: StorageTypeMut<T, R, C>>(
                &self,
                rhs: &MatrixBase<T, R, C, D2>,
                out: &mut MatrixBase<T, R, C, D3>,
            ) {
                // TODO: Simplify this to a shape comparison.
                assert_eq!(self.rows(), rhs.rows());
                assert_eq!(self.cols(), rhs.cols());
                assert_eq!(self.rows(), out.rows());
                assert_eq!(self.cols(), out.cols());

                for i in 0..self.len() {
                    out.data[i] = $op_inner(self.data[i], rhs.data[i]);
                }
            }
        }

        // &Matrix + &Matrix
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageType<T, R, C>,
                D2: StorageType<T, R, C>,
            > $Op<&MatrixBase<T, R, C, D2>> for &MatrixBase<T, R, C, D>
        where
            MatrixNewStorage: NewStorage<T, R, C>,
        {
            type Output = MatrixNew<T, R, C>;

            #[inline]
            fn $op(self, rhs: &MatrixBase<T, R, C, D2>) -> Self::Output {
                let mut out = Self::Output::zero_with_shape(self.rows(), self.cols());
                self.$op_to(rhs, &mut out);
                out
            }
        }

        // &Matrix + Matrix
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageType<T, R, C>,
                D2: StorageType<T, R, C>,
            > $Op<MatrixBase<T, R, C, D2>> for &MatrixBase<T, R, C, D>
        where
            MatrixNewStorage: NewStorage<T, R, C>,
        {
            type Output = MatrixNew<T, R, C>;

            #[inline]
            fn $op(self, rhs: MatrixBase<T, R, C, D2>) -> Self::Output {
                self.$op(&rhs)
            }
        }

        // Matrix + &Matrix
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageType<T, R, C>,
                D2: StorageType<T, R, C>,
            > $Op<&MatrixBase<T, R, C, D2>> for MatrixBase<T, R, C, D>
        where
            MatrixNewStorage: NewStorage<T, R, C>,
        {
            type Output = MatrixNew<T, R, C>;

            #[inline]
            fn $op(mut self, rhs: &MatrixBase<T, R, C, D2>) -> Self::Output {
                (&self).$op(rhs)
            }
        }

        // Matrix + Matrix
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageType<T, R, C>,
                D2: StorageType<T, R, C>,
            > $Op<MatrixBase<T, R, C, D2>> for MatrixBase<T, R, C, D>
        where
            MatrixNewStorage: NewStorage<T, R, C>,
        {
            type Output = MatrixNew<T, R, C>;

            #[inline]
            fn $op(self, rhs: MatrixBase<T, R, C, D2>) -> Self::Output {
                (&self).$op(&rhs)
            }
        }

        // &Matrix + Scalar
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageType<T, R, C>,
                V: num_traits::Num + Copy + Into<T>,
            > $Op<V> for &MatrixBase<T, R, C, D>
        where
            MatrixNewStorage: NewStorage<T, R, C>,
        {
            type Output = MatrixNew<T, R, C>;

            #[inline]
            fn $op(self, rhs: V) -> Self::Output {
                let mut out = Self::Output::zero_with_shape(self.rows(), self.cols());
                for i in 0..(self.rows() * self.cols()) {
                    out.data[i] = $op_inner(self.data[i], rhs.into());
                }

                out
            }
        }

        // Matrix + Scalar
        impl<
                T: ScalarElementType,
                R: Dimension,
                C: Dimension,
                D: StorageType<T, R, C>,
                V: num_traits::Num + Copy + Into<T>,
            > $Op<V> for MatrixBase<T, R, C, D>
        where
            MatrixNewStorage: NewStorage<T, R, C>,
        {
            type Output = MatrixNew<T, R, C>;

            #[inline]
            fn $op(self, rhs: V) -> Self::Output {
                (&self).$op(rhs)
            }
        }
    };
}

cwise_binary_op!(
    AddAssign,
    add_assign,
    AddAssign::add_assign,
    Add,
    add,
    Add::add,
    add_to
);
cwise_binary_op!(
    SubAssign,
    sub_assign,
    SubAssign::sub_assign,
    Sub,
    sub,
    Sub::sub,
    sub_to
);
cwise_binary_op!(
    CwiseMulAssign,
    cwise_mul_assign,
    MulAssign::mul_assign,
    CwiseMul,
    cwise_mul,
    Mul::mul,
    cwise_mul_to
);
cwise_binary_op!(
    CwiseDivAssign,
    cwise_div_assign,
    DivAssign::div_assign,
    CwiseDiv,
    cwise_div,
    Div::div,
    cwise_div_to
);

cwise_binary_op!(
    CwiseMinAssign,
    cwise_min_assign,
    min_assign_impl,
    CwiseMin,
    cwise_min,
    num_traits::real::Real::min,
    cwise_min_to
);

cwise_binary_op!(
    CwiseMaxAssign,
    cwise_max_assign,
    max_assign_impl,
    CwiseMax,
    cwise_max,
    num_traits::real::Real::max,
    cwise_max_to
);

fn min_assign_impl<T: num_traits::real::Real>(value: &mut T, other: T) {
    *value = value.min(other);
}

fn max_assign_impl<T: num_traits::real::Real>(value: &mut T, other: T) {
    *value = value.max(other);
}

// Matrix *= Scalar
impl<
        T: ScalarElementType,
        R: Dimension,
        C: Dimension,
        D: StorageTypeMut<T, R, C>,
        V: num_traits::Num + Copy + Into<T>,
    > MulAssign<V> for MatrixBase<T, R, C, D>
{
    #[inline]
    fn mul_assign(&mut self, rhs: V) {
        self.cwise_mul_assign(rhs);
    }
}

// Matrix * Scalar
impl<
        T: ScalarElementType,
        R: Dimension,
        C: Dimension,
        D: StorageTypeMut<T, R, C>,
        V: num_traits::Num + Copy + Into<T>,
    > Mul<V> for MatrixBase<T, R, C, D>
{
    type Output = Self;

    #[inline]
    fn mul(mut self, rhs: V) -> Self::Output {
        self.mul_assign(rhs);
        self
    }
}

// Matrix /= Scalar.
impl<
        T: ScalarElementType,
        R: Dimension,
        C: Dimension,
        D: StorageTypeMut<T, R, C>,
        V: num_traits::Num + Copy + Into<T>,
    > DivAssign<V> for MatrixBase<T, R, C, D>
{
    #[inline]
    fn div_assign(&mut self, rhs: V) {
        self.cwise_div_assign(rhs);
    }
}

// Matrix / Scalar
impl<
        T: ScalarElementType,
        R: Dimension,
        C: Dimension,
        D: StorageTypeMut<T, R, C>,
        V: num_traits::Num + Copy + Into<T>,
    > Div<V> for MatrixBase<T, R, C, D>
{
    type Output = Self;

    #[inline]
    fn div(mut self, rhs: V) -> Self::Output {
        self.div_assign(rhs);
        self
    }
}
