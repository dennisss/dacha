use crate::matrix::dimension::*;
use crate::matrix::element::ElementType;
use generic_array::{ArrayLength, GenericArray};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Index, IndexMut, Mul};
use typenum::Prod; // TODO: Refactor out this circular reference.

/// A container of elements. Must be indexable in natural contiguous order. Row
/// and column semantics will be defined at a higher level, so the storage type
/// should be agnostic to that.
///
/// There are three main StorageType implementations to be aware of:
/// - Vec: Dynamic contiguous container for a matrix
/// - MatrixInlineStorage: For statically known shapes, stores all elements on
///   the stack with zero allocations.
/// - MatrixBlockStorage: Doesn't directly own any elements, but instead stores
///   a reference to a block of elements in another storage type.
pub trait StorageType<T, R, C>:
    Index<usize, Output = T> + Index<(usize, usize), Output = T>
{
    fn alloc(rows: R, cols: C) -> Self;
    fn rows(&self) -> R;
    fn cols(&self) -> C;
    fn as_ptr(&self) -> *const T; // NOTE: This is only a good thing to do for non-blocky situations.
}

pub trait StorageTypeMut<T, R, C> =
    StorageType<T, R, C> + IndexMut<(usize, usize)> + IndexMut<usize>;

/// Storage for a matrix on the heap.
#[derive(Clone)]
pub struct MatrixDynamicStorage<T, R: Dimension, C: Dimension> {
    data: Vec<T>,
    rows: R,
    cols: C,
}

impl<T: ElementType, R: Dimension, C: Dimension> StorageType<T, R, C>
    for MatrixDynamicStorage<T, R, C>
{
    fn alloc(rows: R, cols: C) -> Self {
        let mut data = vec![];
        data.resize(rows.value() * cols.value(), T::zero());
        Self { data, rows, cols }
    }

    fn rows(&self) -> R {
        self.rows
    }

    fn cols(&self) -> C {
        self.cols
    }

    fn as_ptr(&self) -> *const T {
        self.data.as_ptr()
    }
}

impl<T, R: Dimension, C: Dimension> AsRef<[T]> for MatrixDynamicStorage<T, R, C> {
    fn as_ref(&self) -> &[T] {
        self.data.as_ref()
    }
}

impl<T, R: Dimension, C: Dimension> AsMut<[T]> for MatrixDynamicStorage<T, R, C> {
    fn as_mut(&mut self) -> &mut [T] {
        self.data.as_mut()
    }
}

impl<T, R: Dimension, C: Dimension> Index<usize> for MatrixDynamicStorage<T, R, C> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl<T, R: Dimension, C: Dimension> Index<(usize, usize)> for MatrixDynamicStorage<T, R, C> {
    type Output = T;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        &self.data[index.0 * self.cols.value() + index.1]
    }
}

impl<T, R: Dimension, C: Dimension> IndexMut<usize> for MatrixDynamicStorage<T, R, C> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}

impl<T, R: Dimension, C: Dimension> IndexMut<(usize, usize)> for MatrixDynamicStorage<T, R, C> {
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        &mut self.data[index.0 * self.cols.value() + index.1]
    }
}

/// Storage which stores all elements in stack memory for statically
/// allocatable sizes.
#[derive(Clone)]
// #[repr(packed)]
pub struct MatrixInlineStorage<T, R: StaticDim, C: StaticDim, S: ArrayLength<T>> {
    data: GenericArray<T, S>,
    r: PhantomData<R>,
    c: PhantomData<C>,
}

impl<T, R: StaticDim, C: StaticDim, S: ArrayLength<T>> AsRef<[T]>
    for MatrixInlineStorage<T, R, C, S>
{
    fn as_ref(&self) -> &[T] {
        self.data.as_ref()
    }
}

impl<T, R: StaticDim, C: StaticDim, S: ArrayLength<T>> AsMut<[T]>
    for MatrixInlineStorage<T, R, C, S>
{
    fn as_mut(&mut self) -> &mut [T] {
        self.data.as_mut()
    }
}

impl<T, R: StaticDim, C: StaticDim, S: ArrayLength<T>> std::ops::Index<usize>
    for MatrixInlineStorage<T, R, C, S>
{
    type Output = T;
    fn index(&self, idx: usize) -> &T {
        // TODO: Whenever we do this, we need to verify that the column index isn't out
        // of range (can result in skipping rows)
        &self.data.as_ref()[idx]
    }
}

impl<T, R: StaticDim, C: StaticDim, S: ArrayLength<T>> std::ops::Index<(usize, usize)>
    for MatrixInlineStorage<T, R, C, S>
{
    type Output = T;
    fn index(&self, idx: (usize, usize)) -> &T {
        // TODO: Whenever we do this, we need to verify that the column index isn't out
        // of range (can result in skipping rows)
        &self.data.as_ref()[C::to_usize() * idx.0 + idx.1]
    }
}

impl<T, R: StaticDim, C: StaticDim, S: ArrayLength<T>> std::ops::IndexMut<usize>
    for MatrixInlineStorage<T, R, C, S>
{
    fn index_mut(&mut self, idx: usize) -> &mut T {
        &mut self.data.as_mut()[idx]
    }
}

impl<T, R: StaticDim, C: StaticDim, S: ArrayLength<T>> std::ops::IndexMut<(usize, usize)>
    for MatrixInlineStorage<T, R, C, S>
{
    fn index_mut(&mut self, idx: (usize, usize)) -> &mut T {
        &mut self.data.as_mut()[C::to_usize() * idx.0 + idx.1]
    }
}

impl<T: Default, R: StaticDim, C: StaticDim, S: ArrayLength<T>> StorageType<T, R, C>
    for MatrixInlineStorage<T, R, C, S>
{
    fn alloc(rows: R, cols: C) -> Self {
        Self {
            data: GenericArray::default(),
            r: PhantomData,
            c: PhantomData,
        }
    }

    fn rows(&self) -> R {
        R::default()
    }

    fn cols(&self) -> C {
        C::default()
    }

    fn as_ptr(&self) -> *const T {
        self.data.as_ptr()
    }
}

/// A container of referenced elements used to represent a chunk of another
/// matrix inplace.
pub struct MatrixBlockStorage<'a, T, Tp: AsRef<[T]> + 'a, R: Dimension, C: Dimension, S: Dimension>
{
    // TODO: Eventually make all fields here private.
    pub data: Tp,

    pub rows: R,

    /// Number of contiguous elements before there is a gap to the next line of
    /// elements.
    pub cols: C,
    /// Total size of a single line of elements (= width + padding).
    pub stride: S,

    pub lifetime: PhantomData<&'a T>,
}

impl<'a, T, Tp: AsRef<[T]> + 'a, R: Dimension, C: Dimension, S: Dimension>
    MatrixBlockStorage<'a, T, Tp, R, C, S>
{
    /// Gets the index into the backing slice given an index relative to the
    /// outer rows/cols size
    fn inner_index(&self, idx: usize) -> usize {
        (idx / self.cols.value()) * self.stride.value() + (idx % self.cols.value())
    }
}

impl<'a, T, Tp: AsRef<[T]> + 'a, R: Dimension, C: Dimension, S: Dimension> StorageType<T, R, C>
    for MatrixBlockStorage<'a, T, Tp, R, C, S>
{
    fn alloc(rows: R, cols: C) -> Self {
        panic!("Can not allocate matrix blocks");
    }

    fn rows(&self) -> R {
        self.rows
    }

    fn cols(&self) -> C {
        self.cols
    }

    fn as_ptr(&self) -> *const T {
        self.data.as_ref().as_ptr()
    }
}

impl<'a, T, Tp: AsRef<[T]> + 'a, R: Dimension, C: Dimension, S: Dimension> Index<usize>
    for MatrixBlockStorage<'a, T, Tp, R, C, S>
{
    type Output = T;
    fn index(&self, idx: usize) -> &T {
        &self.data.as_ref()[self.inner_index(idx)]
    }
}

impl<'a, T, Tp: AsRef<[T]> + AsMut<[T]> + 'a, R: Dimension, C: Dimension, S: Dimension>
    IndexMut<usize> for MatrixBlockStorage<'a, T, Tp, R, C, S>
{
    fn index_mut(&mut self, idx: usize) -> &mut T {
        let i = self.inner_index(idx);
        &mut self.data.as_mut()[i]
    }
}

impl<'a, T, Tp: AsRef<[T]> + 'a, R: Dimension, C: Dimension, S: Dimension> Index<(usize, usize)>
    for MatrixBlockStorage<'a, T, Tp, R, C, S>
{
    type Output = T;
    fn index(&self, idx: (usize, usize)) -> &T {
        &self.data.as_ref()[idx.0 * self.stride.value() + idx.1]
    }
}

impl<'a, T, Tp: AsRef<[T]> + AsMut<[T]> + 'a, R: Dimension, C: Dimension, S: Dimension>
    IndexMut<(usize, usize)> for MatrixBlockStorage<'a, T, Tp, R, C, S>
{
    fn index_mut(&mut self, idx: (usize, usize)) -> &mut T {
        &mut self.data.as_mut()[idx.0 * self.stride.value() + idx.1]
    }
}

pub struct MatrixTransposeStorage<
    'a,
    T,
    R: Dimension,
    C: Dimension,
    S: StorageType<T, C, R>,
    Sp: Deref<Target = S> + 'a,
> {
    pub inner: Sp,
    pub t: PhantomData<T>,
    pub r: PhantomData<R>,
    pub c: PhantomData<C>,
    pub s: PhantomData<&'a S>,
}

impl<'a, T, R: Dimension, C: Dimension, S: StorageType<T, C, R>, Sp: Deref<Target = S> + 'a>
    StorageType<T, R, C> for MatrixTransposeStorage<'a, T, R, C, S, Sp>
{
    fn alloc(rows: R, cols: C) -> Self {
        panic!("Can not allocate matrix transpose");
    }

    fn rows(&self) -> R {
        self.inner.cols()
    }

    fn cols(&self) -> C {
        self.inner.rows()
    }

    fn as_ptr(&self) -> *const T {
        panic!("Can not use a transpose storage as a pointer");
    }
}
impl<'a, T, R: Dimension, C: Dimension, S: StorageType<T, C, R>, Sp: Deref<Target = S> + 'a>
    Index<usize> for MatrixTransposeStorage<'a, T, R, C, S, Sp>
{
    type Output = T;
    fn index(&self, idx: usize) -> &T {
        let i = idx / self.cols().value();
        let j = idx % self.rows().value();
        &self[(i, j)]
    }
}

impl<
        'a,
        T,
        R: Dimension,
        C: Dimension,
        S: StorageTypeMut<T, C, R>,
        Sp: Deref<Target = S> + DerefMut + 'a,
    > IndexMut<usize> for MatrixTransposeStorage<'a, T, R, C, S, Sp>
{
    fn index_mut(&mut self, idx: usize) -> &mut T {
        let i = idx / self.cols().value();
        let j = idx % self.rows().value();
        &mut self[(i, j)]
    }
}

impl<'a, T, R: Dimension, C: Dimension, S: StorageType<T, C, R>, Sp: Deref<Target = S> + 'a>
    Index<(usize, usize)> for MatrixTransposeStorage<'a, T, R, C, S, Sp>
{
    type Output = T;
    fn index(&self, idx: (usize, usize)) -> &T {
        &self.inner[(idx.1, idx.0)]
    }
}

impl<
        'a,
        T,
        R: Dimension,
        C: Dimension,
        S: StorageTypeMut<T, C, R>,
        Sp: Deref<Target = S> + DerefMut + 'a,
    > IndexMut<(usize, usize)> for MatrixTransposeStorage<'a, T, R, C, S, Sp>
{
    fn index_mut(&mut self, idx: (usize, usize)) -> &mut T {
        &mut self.inner[(idx.1, idx.0)]
    }
}

pub struct MatrixNewStorage;

/// Helper for looking up the best storage type for representing a matrix based
/// on the specified shape.
pub trait NewStorage<T, R, C> {
    // All new storage types will store elements contiguously so we can always
    // take contiguous slices of their contents.
    type Type: StorageTypeMut<T, R, C> + AsMut<[T]> + AsRef<[T]> + Clone;
}

impl<T: ElementType> NewStorage<T, Dynamic, Dynamic> for MatrixNewStorage {
    type Type = MatrixDynamicStorage<T, Dynamic, Dynamic>;
}
impl<T: ElementType, C: StaticDim> NewStorage<T, Dynamic, C> for MatrixNewStorage {
    type Type = MatrixDynamicStorage<T, Dynamic, C>;
}
impl<T: ElementType, R: StaticDim> NewStorage<T, R, Dynamic> for MatrixNewStorage {
    type Type = MatrixDynamicStorage<T, R, Dynamic>;
}
impl<T: ElementType, R: StaticDim + Mul<C>, C: StaticDim> NewStorage<T, R, C> for MatrixNewStorage
where
    <R as Mul<C>>::Output: ArrayLength<T> + Clone,
{
    type Type = MatrixInlineStorage<T, R, C, Prod<R, C>>;
}

// impl<T: Default, R: Dimension, C: Dimension> NewStorage<T> for (R, C)
// where (C, R): NewStorage<T> {
// 	type Type = <(C, R) as NewStorage<T>>::Type;
// }
