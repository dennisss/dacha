use core::marker::PhantomData;
use core::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

use generic_array::{ArrayLength, GenericArray};
use typenum::{Prod, Unsigned, U1, U2, U3, U4, U5, U8};

use crate::argmax::argmax;
use crate::matrix::cwise_binary_ops::CwiseDivAssign;
use crate::matrix::dimension::*;
use crate::matrix::element::*;
use crate::matrix::storage::*;
use crate::number::Cast;
use crate::number::{Min, One, Zero};

/*
    TODO: Needed operations:
    - AddAssign/SubAssign
    - Mul by a scalar
    - Eigenvector decomposition

    -


*/

#[derive(Clone)]
pub struct MatrixBase<T, R: Dimension, C: Dimension, S: StorageType<T, R, C>> {
    pub(super) data: S,
    t: PhantomData<T>,
    r: PhantomData<R>,
    c: PhantomData<C>,
}

pub type MatrixBlock<'a, T, R, C, S> =
    MatrixBase<T, R, C, MatrixBlockStorage<'a, T, &'a [T], R, C, S>>;
pub type MatrixBlockMut<'a, T, R, C, S> =
    MatrixBase<T, R, C, MatrixBlockStorage<'a, T, &'a mut [T], R, C, S>>;

// TODO: This is probably wrong?
// pub type MatrixTranspose<'a, T, R, C, S> =
//     MatrixBase<T, R, C, MatrixBlockStorage<'a, T, &'a [T], C, R, S>>;

pub type MatrixStatic<T, R: Mul<C>, C> =
    MatrixBase<T, R, C, MatrixInlineStorage<T, R, C, Prod<R, C>>>;
pub type VectorStatic<T, R> = MatrixBase<T, R, U1, MatrixInlineStorage<T, R, U1, Prod<R, U1>>>;

pub type VectorBase<T, R, D> = MatrixBase<T, R, U1, D>;

#[cfg(feature = "alloc")]
pub type Matrix<T, R, C> = MatrixBase<T, R, C, MatrixDynamicStorage<T, R, C>>;

pub type Matrix2<T> = MatrixStatic<T, U2, U2>;
pub type Matrix2i = MatrixStatic<isize, U2, U2>;
pub type Matrix2f = MatrixStatic<f32, U2, U2>;
pub type Matrix3f = MatrixStatic<f32, U3, U3>;
pub type Matrix3d = MatrixStatic<f64, U3, U3>;
pub type Matrix4f = MatrixStatic<f32, U4, U4>;
pub type Matrix4d = MatrixStatic<f64, U4, U4>;
pub type Matrix8f = MatrixStatic<f32, U8, U8>;
#[cfg(feature = "alloc")]
pub type MatrixXd = Matrix<f64, Dynamic, Dynamic>;

#[cfg(feature = "alloc")]
pub type Vector<T, R> = Matrix<T, R, U1>;

pub type Vector2<T> = VectorStatic<T, U2>;
pub type Vector2i = VectorStatic<isize, U2>;
pub type Vector2i64 = VectorStatic<i64, U2>;
pub type Vector2u = VectorStatic<usize, U2>;
pub type Vector2f = VectorStatic<f32, U2>;
pub type Vector2d = VectorStatic<f64, U2>;
pub type Vector3<T> = VectorStatic<T, U3>;
pub type Vector3u = VectorStatic<usize, U3>;
pub type Vector3f = VectorStatic<f32, U3>;
pub type Vector4f = VectorStatic<f32, U4>;
pub type Vector4d = VectorStatic<f64, U4>;

/// Special alias for selecting the best storage for the given matrix shape.
pub type MatrixNew<T, R, C> = MatrixBase<T, R, C, <MatrixNewStorage as NewStorage<T, R, C>>::Type>;

// TODO: U1 should always be a trivial case.
pub type VectorNew<T, R> = MatrixBase<T, R, U1, <MatrixNewStorage as NewStorage<T, R, U1>>::Type>;

impl<T: ElementType, R: Dimension, C: Dimension, Data: StorageTypeMut<T, R, C>>
    MatrixBase<T, R, C, Data>
{
    // Creates an empty matrix with a dynamic size.
    pub(super) fn new_with_shape(rows: usize, cols: usize) -> Self {
        // Any static dimensions must match the given dimension.
        // if let Some(r) = R::dim() {
        //     assert_eq!(rows, r);
        // }
        // if let Some(c) = C::dim() {
        //     assert_eq!(cols, c);
        // }

        let data = Data::alloc(R::from_usize(rows), C::from_usize(cols));
        Self {
            data,
            r: PhantomData,
            c: PhantomData,
            t: PhantomData,
        }
    }

    pub fn zero_with_shape(rows: usize, cols: usize) -> Self {
        Self::new_with_shape(rows, cols)
    }

    // TODO: For static dimensions, we need them to match?
    pub fn copy_from<Data2: StorageType<T, R, C>>(&mut self, other: &MatrixBase<T, R, C, Data2>) {
        assert_eq!(self.rows(), other.rows());
        assert_eq!(self.cols(), other.cols());
        for i in 0..(self.rows() * self.cols()) {
            self.data[i] = other.data[i];
        }
    }

    pub fn copy_from_slice(&mut self, other: &[T]) {
        assert_eq!(self.rows() * self.cols(), other.len());
        for i in 0..other.len() {
            self.data[i] = other[i];
        }
    }

    pub fn null() -> Self {
        let r = if let Some(r) = R::dim() { r } else { 0 };
        let c = if let Some(c) = C::dim() { c } else { 0 };
        Self::zero_with_shape(r, c)
    }
}

impl core::convert::From<(Vector2f, f32)> for Vector3f {
    fn from((a, b): (Vector2f, f32)) -> Self {
        Self::from_slice(&[a.x(), a.y(), b])
    }
}

// Matrix3d::zero().inverse()

impl<T: ElementType, R: StaticDim, C: StaticDim, D: StorageTypeMut<T, R, C>>
    MatrixBase<T, R, C, D>
{
    /// Creates an empty matrix with a statically defined size.
    fn new() -> Self {
        Self::new_with_shape(R::dim().unwrap(), C::dim().unwrap())
    }

    pub fn zero() -> Self {
        Self::new()
    }
}

impl<T: ElementType + One, R: Dimension, C: Dimension, D: StorageTypeMut<T, R, C>>
    MatrixBase<T, R, C, D>
{
    pub fn identity_with_shape(rows: usize, cols: usize) -> Self {
        let mut m = Self::zero_with_shape(rows, cols);
        for i in 0..Min::min(rows, cols) {
            m[(i, i)] = T::one();
        }
        m
    }
}

impl<T: ElementType + One, R: StaticDim, C: StaticDim, D: StorageTypeMut<T, R, C>>
    MatrixBase<T, R, C, D>
{
    pub fn identity() -> Self {
        Self::identity_with_shape(R::dim().unwrap(), C::dim().unwrap())
    }
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T, R, C>> MatrixBase<T, R, C, D> {
    /// Create a new matrix with elements casted to another type.
    pub fn cast<Y: 'static + ElementType>(&self) -> MatrixNew<Y, R, C>
    where
        T: Cast<Y>,
        MatrixNewStorage: NewStorage<Y, R, C>,
    {
        let mut out = MatrixNew::<Y, R, C>::new_with_shape(self.rows(), self.cols());
        for i in 0..self.len() {
            out[i] = self[i].cast();
        }
        out
    }
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T, R, C> + AsRef<[T]>>
    MatrixBase<T, R, C, D>
{
    pub fn block_with_shape<RB: Dimension, CB: Dimension>(
        &self,
        row: usize,
        col: usize,
        row_height: usize,
        col_width: usize,
    ) -> MatrixBlock<T, RB, CB, C> {
        // if let Some(r) = RB::dim() {
        //     assert_eq!(row_height, r);
        // }
        // if let Some(c) = CB::dim() {
        //     assert_eq!(col_width, c);
        // }
        assert!(row + row_height <= self.rows());
        assert!(col + col_width <= self.cols());
        assert!(row_height > 0);
        let start = row * self.cols() + col;
        let end = start + ((row_height - 1) * self.cols() + col_width);
        MatrixBase {
            // XXX: Here we may want to either
            data: MatrixBlockStorage {
                data: &self.data.as_ref()[start..end],
                rows: RB::from_usize(row_height), // NOTE: from_usize has an assertion in it,
                cols: CB::from_usize(col_width),
                stride: self.data.cols(),
                lifetime: PhantomData,
            },
            t: PhantomData,
            r: PhantomData,
            c: PhantomData,
        }
    }

    pub fn block<RB: StaticDim, CB: StaticDim>(
        &self,
        row: usize,
        col: usize,
    ) -> MatrixBlock<T, RB, CB, C> {
        self.block_with_shape(row, col, RB::dim().unwrap(), CB::dim().unwrap())
    }

    pub fn col(&self, col: usize) -> MatrixBlock<T, R, U1, C> {
        self.block_with_shape(0, col, self.rows(), 1)
    }

    pub fn row(&self, row: usize) -> MatrixBlock<T, U1, C, C> {
        self.block_with_shape(row, 0, 1, self.cols())
    }
}

impl<
        T: ElementType,
        R: Dimension,
        C: Dimension,
        D: StorageTypeMut<T, R, C> + AsRef<[T]> + AsMut<[T]>,
    > MatrixBase<T, R, C, D>
{
    // TODO: Dedup with above
    pub fn block_with_shape_mut<RB: Dimension, CB: Dimension>(
        &mut self,
        row: usize,
        col: usize,
        row_height: usize,
        col_width: usize,
    ) -> MatrixBlockMut<T, RB, CB, C> {
        // if let Some(r) = RB::dim() {
        //     assert_eq!(row_height, r);
        // }
        // if let Some(c) = CB::dim() {
        //     assert_eq!(col_width, c);
        // }
        assert!(row + row_height <= self.rows());
        assert!(col + col_width <= self.cols());
        assert!(row_height > 0);
        let start = row * self.cols() + col;
        let end = start + ((row_height - 1) * self.cols() + col_width);

        let stride = self.data.cols();
        MatrixBase {
            // XXX: Here we may want to either
            data: MatrixBlockStorage {
                data: &mut self.data.as_mut()[start..end],
                rows: RB::from_usize(row_height),
                cols: CB::from_usize(col_width),
                stride,
                lifetime: PhantomData,
            },
            t: PhantomData,
            r: PhantomData,
            c: PhantomData,
        }

        //		unsafe {
        //			core::mem::transmute(self.block_with_shape::<RB, CB>(
        //				row, col, row_height, col_width))
        //		}
    }

    pub fn block_mut<RB: StaticDim, CB: StaticDim>(
        &mut self,
        row: usize,
        col: usize,
    ) -> MatrixBlockMut<T, RB, CB, C> {
        self.block_with_shape_mut(row, col, RB::dim().unwrap(), CB::dim().unwrap())
    }

    pub fn col_mut(&mut self, col: usize) -> MatrixBlockMut<T, R, U1, C> {
        self.block_with_shape_mut(0, col, self.rows(), 1)
    }
}

// as_transpose
// transpose_inplace
// transposed()
// transpose() <-

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T, R, C> + AsRef<[T]>>
    MatrixBase<T, R, C, D>
{
    /// Constructs a new matrix which references the same data as the current
    /// matrix, but operates as if it were transposed.
    pub fn as_transpose<'a>(
        &'a self,
    ) -> MatrixBase<T, C, R, MatrixTransposeStorage<'a, T, C, R, D, &'a D>> {
        // TODO: A transpose of a transpose should become a no-op.
        MatrixBase {
            // XXX: Here we may want to either
            data: MatrixTransposeStorage {
                inner: &self.data,
                t: PhantomData,
                r: PhantomData,
                c: PhantomData,
                s: PhantomData,
            },
            t: PhantomData,
            r: PhantomData,
            c: PhantomData,
        }
    }

    pub fn transpose(&self) -> MatrixNew<T, C, R>
    where
        MatrixNewStorage: NewStorage<T, C, R>,
    {
        let mut out = MatrixNew::zero_with_shape(self.cols(), self.rows());
        for i in 0..out.rows() {
            for j in 0..out.cols() {
                out[(i, j)] = self[(j, i)];
            }
        }
        out
    }
}

impl<T, R: Dimension, C: Dimension, D: StorageType<T, R, C>> MatrixBase<T, R, C, D> {
    pub fn len(&self) -> usize {
        self.rows() * self.cols()
    }

    #[inline]
    pub fn cols(&self) -> usize {
        self.data.cols().value()
    }

    #[inline]
    pub fn rows(&self) -> usize {
        self.data.rows().value()
    }
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageType<T, R, C>> MatrixBase<T, R, C, D> {
    // TODO: Only implement for vectors with a shape known to be big enough
    pub fn x(&self) -> T {
        self[0]
    }
    pub fn y(&self) -> T {
        self[1]
    }
    pub fn z(&self) -> T {
        self[2]
    }
    pub fn w(&self) -> T {
        self[3]
    }

    pub fn to_owned(&self) -> MatrixNew<T, R, C>
    where
        MatrixNewStorage: NewStorage<T, R, C>,
    {
        let mut out = MatrixNew::<T, R, C>::zero_with_shape(self.rows(), self.cols());
        out.copy_from(self);
        out
    }
}

impl<T: ElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T, R, C>>
    MatrixBase<T, R, C, D>
{
    pub fn from_slice_with_shape(rows: usize, cols: usize, data: &[T]) -> Self {
        let mut mat = Self::new_with_shape(rows, cols);

        // TODO: Make this more efficient.
        assert_eq!(data.len(), mat.rows() * mat.cols());
        for i in 0..data.len() {
            mat.data[i] = data[i];
        }

        //		assert_eq!(mat.data.len(), data.len());
        //		mat.data.clone_from_slice(data);

        mat
    }

    /// TODO: Must be a square matrix (some of this will be guranteed by
    /// zero_with_shape though).
    pub fn diag(data: &[T]) -> Self {
        Self::diag_with_shape(data.len(), data.len(), data)
    }

    pub fn diag_with_shape(rows: usize, cols: usize, data: &[T]) -> Self {
        assert_eq!(data.len(), core::cmp::min(rows, cols));
        let mut m = Self::zero_with_shape(rows, cols);
        for i in 0..data.len() {
            m[(i, i)] = data[i];
        }
        m
    }
}

impl<T: ElementType, R: StaticDim, C: StaticDim, D: StorageTypeMut<T, R, C>>
    MatrixBase<T, R, C, D>
{
    pub fn from_slice(data: &[T]) -> Self {
        Self::from_slice_with_shape(R::dim().unwrap(), C::dim().unwrap(), data)
    }

    pub fn as_ptr(&self) -> *const T {
        self.data.as_ptr()
    }
}

impl<T, R: Dimension, C: Dimension, D: StorageType<T, R, C>> core::ops::Index<usize>
    for MatrixBase<T, R, C, D>
{
    type Output = T;

    #[inline]
    fn index(&self, i: usize) -> &Self::Output {
        &self.data[i]
    }
}

impl<T, R: Dimension, C: Dimension, D: StorageTypeMut<T, R, C>> core::ops::IndexMut<usize>
    for MatrixBase<T, R, C, D>
{
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut Self::Output {
        &mut self.data[i]
    }
}

impl<T, R: Dimension, C: Dimension, D: StorageType<T, R, C>> core::ops::Index<(usize, usize)>
    for MatrixBase<T, R, C, D>
{
    type Output = T;

    #[inline]
    fn index(&self, ij: (usize, usize)) -> &Self::Output {
        assert!(ij.1 < self.cols());
        &self.data[ij]
    }
}

impl<T, R: Dimension, C: Dimension, D: StorageTypeMut<T, R, C>> core::ops::IndexMut<(usize, usize)>
    for MatrixBase<T, R, C, D>
{
    #[inline]
    fn index_mut(&mut self, ij: (usize, usize)) -> &mut T {
        let cols = self.cols();
        assert!(ij.1 < cols);
        self.data.index_mut(ij)
    }
}

impl<T, R: Dimension, C: Dimension, D: StorageType<T, R, C> + AsRef<[T]>> AsRef<[T]>
    for MatrixBase<T, R, C, D>
{
    fn as_ref(&self) -> &[T] {
        self.data.as_ref()
    }
}

impl<T, R: Dimension, C: Dimension, D: StorageTypeMut<T, R, C> + AsMut<[T]>> AsMut<[T]>
    for MatrixBase<T, R, C, D>
{
    fn as_mut(&mut self) -> &mut [T] {
        self.data.as_mut()
    }
}

////////////////////////////////////////////////////////////////////////////////

impl<T: ElementType + Mul<T, Output = T> + Sub<T, Output = T>, D: StorageType<T, U3, U1>>
    MatrixBase<T, U3, U1, D>
{
    /// TODO: Also have an inplace version and a version that assigns into an
    /// existing buffer.
    pub fn cross<D2: StorageType<T, U3, U1>>(
        &self,
        rhs: &MatrixBase<T, U3, U1, D2>,
    ) -> VectorStatic<T, U3> {
        VectorStatic::<T, U3>::from_slice(&[
            self.y() * rhs.z() - self.z() * rhs.y(),
            self.z() * rhs.x() - self.x() * rhs.z(),
            self.x() * rhs.y() - self.y() * rhs.x(),
        ])
    }
}

impl<T: ScalarElementType, R: Dimension, C: Dimension, D: StorageType<T, R, C>>
    MatrixBase<T, R, C, D>
{
    pub fn max_value(&self) -> T {
        let mut max = self[0];
        for i in 1..(self.rows() * self.cols()) {
            if self[i] > max {
                max = self[i];
            }
        }

        max
    }

    pub fn min_value(&self) -> T {
        let mut min = self[0];
        for i in 1..(self.rows() * self.cols()) {
            if self[i] < min {
                min = self[i];
            }
        }

        min
    }

    /// Computes the inner product with another matrix.
    ///
    /// The dimensions must exactly match. If you want to perform a dot product
    /// between matrices of different shapes, then you should explicitly reshape
    /// them to be the same shape.
    pub fn dot<R2: Dimension, C2: Dimension, D2: StorageType<T, R2, C2>>(
        &self,
        rhs: &MatrixBase<T, R2, C2, D2>,
    ) -> T
    where
        (R, R2): MaybeEqualDims,
        (C, C2): MaybeEqualDims,
    {
        assert_eq!(self.rows(), rhs.rows());
        assert_eq!(self.cols(), rhs.cols());

        let mut out = T::zero();
        for i in 0..self.rows() * self.cols() {
            out += self[i] * rhs[i];
        }

        out
    }

    /// Computes the product of all entries on the diagonal
    /// NOTE: Assumes that the matrix is square.
    fn diagonal_product(&self) -> T {
        let mut v = T::one();
        for i in 0..self.rows() {
            v *= self[(i, i)];
        }

        v
    }

    pub fn is_square(&self) -> bool {
        self.rows() == self.cols()
    }

    /*
    pub fn is_zero(&self) -> bool {

    }

    pub fn is_identity(&self) -> bool {

    }



    // TODO: Should be able to make random matrices and random matrics with
    // symmetry, etc.

    pub fn is_triangular(&self, upper: bool) -> bool {

    }

    pub fn is_bitriangular(&self) -> bool {

    }

    pub fn is_orthogonal(&self) -> bool {
        (self * self.transpose()).is_identity()
    }
    */
}

impl<T: FloatElementType, R: Dimension, C: Dimension, D: StorageType<T, R, C>>
    MatrixBase<T, R, C, D>
{
    pub fn is_symmetric(&self) -> bool {
        if !self.is_square() {
            // TODO: Can it be symmetric when not square?
            return false;
        }

        for i in 0..self.rows() {
            for j in 0..i {
                // TODO: Use aprpoximate equality here.
                if self[(i, j)] != self[(j, i)] {
                    return false;
                }
            }
        }

        true
    }

    pub fn is_diagonal(&self) -> bool {
        for i in 0..self.rows() {
            for j in 0..self.cols() {
                if i == j {
                    continue;
                }
                if self[(i, j)].abs() > T::error_epsilon() {
                    return false;
                }
            }
        }

        true
    }

    /// Computes if the upper and lower entries of the matrix are
    fn is_upper_lower_zero(&self) -> (bool, bool) {
        let mut upper = true;
        let mut lower = true;

        for i in 0..self.rows() {
            for j in 0..self.cols() {
                let zero = self[(i, j)].approx_zero();
                if i > j {
                    lower &= zero;
                } else if i < j {
                    upper &= zero;
                }
            }
        }

        (upper, lower)
    }

    pub fn is_upper_triangular(&self) -> bool {
        self.is_upper_lower_zero().1
    }

    pub fn is_lower_triangular(&self) -> bool {
        self.is_upper_lower_zero().0
    }

    pub fn is_triangular(&self) -> bool {
        let (u, l) = self.is_upper_lower_zero();
        u || l
    }

    pub fn norm_squared(&self) -> T {
        let mut out = T::zero();
        for i in 0..(self.rows() * self.cols()) {
            let v = self[i];
            out += v * v;
        }

        out
    }

    pub fn norm(&self) -> T {
        self.norm_squared().sqrt()
    }

    // TODO: Must optionally return if it doesn't have an inverse
    pub fn inverse(&self) -> MatrixNew<T, R, C>
    where
        C: MulDims<U2>,
        MatrixNewStorage: NewStorage<T, R, C>,
        MatrixNewStorage: NewStorage<T, R, ProdDims<C, U2>>,
    {
        assert_eq!(self.rows(), self.cols());

        // Form matrix [ self, Identity ].
        let mut m =
            MatrixNew::<T, R, ProdDims<C, U2>>::new_with_shape(self.rows(), 2 * self.cols());
        m.block_with_shape_mut::<R, C>(0, 0, self.rows(), self.cols())
            .copy_from(self);
        m.block_with_shape_mut::<R, C>(0, self.cols(), self.rows(), self.cols())
            .copy_from(&MatrixNew::<T, R, C>::identity_with_shape(
                self.rows(),
                self.cols(),
            ));

        m.gaussian_elimination();

        // Return right half of the matrix.
        // TODO: Support inverting in-place by copying back from the temp matrix
        // above.
        let mut inv = MatrixBase::new_with_shape(self.rows(), self.cols());
        inv.copy_from(&m.block_with_shape(0, self.cols(), self.rows(), self.cols()));
        inv
    }

    pub fn determinant(&self) -> T
    where
        MatrixNewStorage: NewStorage<T, R, C>,
    {
        assert!(self.is_square());

        if self.rows() == 1 {
            return self.data[0].clone();
        } else if self.rows() == 2 {
            return self[(0, 0)] * self[(1, 1)] - self[(0, 1)] * self[(1, 0)];
        }
        // TODO: Add special 3x3 case
        else if self.is_triangular() {
            // The determinant of an upper or lower triangular matrix is the
            // product of the diagonal entries.
            self.diagonal_product()
        } else {
            // Reduce matrix to upper triangular.
            let mut m = self.to_owned();
            m.gaussian_elimination();
            m.diagonal_product()
        }
    }

    pub fn is_normalized(&self) -> bool {
        (T::one() - self.norm_squared()).approx_zero()
    }
}

impl<T: FloatElementType, R: Dimension, C: Dimension, D: StorageTypeMut<T, R, C>>
    MatrixBase<T, R, C, D>
{
    /// Normalizes the matrix in place. Does nothing if the norm is near zero.
    pub fn normalize(&mut self) {
        let n = self.norm();
        if n.approx_zero() {
            return;
        }

        self.cwise_div_assign(n);
    }

    pub fn normalized(mut self) -> Self {
        self.normalize();
        self
    }

    pub fn sqrt(mut self) -> Self {
        for i in 0..(self.rows() * self.cols()) {
            self[i] = self[i].sqrt();
        }
        self
    }

    pub fn swap_rows(&mut self, i1: usize, i2: usize) {
        if i1 == i2 {
            return;
        }

        for j in 0..self.cols() {
            let temp = self[(i1, j)]; // TODO: Cache this reference.
            self[(i1, j)] = self[(i2, j)];
            self[(i2, j)] = temp;
        }
    }

    pub fn swap_cols(&mut self, j1: usize, j2: usize) {
        if j1 == j2 {
            return;
        }

        for i in 0..self.rows() {
            let temp = self[(i, j1)];
            self[(i, j1)] = self[(i, j2)];
            self[(i, j2)] = temp;
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
    pub fn gaussian_elimination(&mut self) {
        let mut h = 0; // Current pivot row.
        let mut k = 0; // Current pivot column.

        while h < self.rows() && k < self.cols() {
            // Find row index with highest value in the current column.
            let i_max = argmax(h..self.rows(), |i| self[(i, k)].abs()).unwrap();

            if self[(i_max, k)].approx_zero() {
                // This column has no pivot.
                k += 1
            } else {
                self.swap_rows(h, i_max);

                // Normalize the pivot row.
                let s = T::one() / self[(h, k)];
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
                    for j in (k + 1)..self.cols() {
                        self[(i, j)] = self[(i, j)] - f * self[(h, j)];
                    }
                }

                h += 1;
                k += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        // println!("HELLO WORLD");
        // println!("{:?}", MatrixXd::from_slice_with_shape(2, 2, &[1.0, 2.0,
        // 4.0, 5.0]));
    }

    #[test]
    fn inverse() {
        let m = Matrix3d::from_slice(&[0.0, 0.2, 0.0, 0.5, 1.0, 0.0, 1.0, 0.0, 0.1]);

        let mi = m.inverse();

        println!("{:?}", mi);

        println!("{:?}", m * mi);
    }

    #[test]
    fn matrix_sub() {
        let m = Matrix3d::from_slice(&[1.0, 4.0, 9.0, 2.0, 5.0, 8.0, 3.0, 6.0, 7.0]);
        let m2 = Matrix3d::from_slice(&[2.0, 4.0, 9.0, 2.0, 0.0, 10.0, 1.0, 6.0, 0.0]);

        println!("{:?}", m - m2)
    }

    #[test]
    fn matrix_static_size() {
        assert_eq!(core::mem::size_of::<Vector2i>(), 16);
        assert_eq!(core::mem::size_of::<Vector3f>(), 12);
        assert_eq!(core::mem::size_of::<Vector4f>(), 16);
        assert_eq!(core::mem::size_of::<Vector4d>(), 32);
    }
}
