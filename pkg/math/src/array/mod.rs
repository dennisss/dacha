mod broadcast;

use alloc::string::ToString;
use alloc::vec::Vec;
use core::fmt::Debug;
use core::ops::{Add, AddAssign, Div, Mul, MulAssign, Sub, SubAssign};

use crate::array::broadcast::*;
use crate::number::{Cast, One, Zero};

// TODO: First implement an ArrayMut which has operations that are allowed to
// mutate the vector in it.

// Then implement regular Array as a Ref counted ArrayMut

// Ideally

// TODO: An empty shape should imply a scalar

// N-dimensional array of data with known type.
#[derive(Clone)]
pub struct Array<T> {
    // TODO: Make these private.
    pub shape: Vec<usize>,
    pub data: Vec<T>, // TODO: Could use general scalar multiplication, addition, division
}

impl<T: Debug> Debug for Array<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let shape = self
            .shape
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        write!(f, "{{{}}}{:?}", shape, &self.data[..])
    }
}

// impl<T: Copy> Copy for Array<T> where T: Copy {}

pub enum ArrayIndex {
    All,
    One(usize),
    Range(usize, usize),
}

impl<T> Array<T> {
    // Want to have a random generation
    // A zeros
    // A ones
    // reshape (already below)
    // cast
    // tile
    // random (uniform or something else)

    pub fn scalar(value: T) -> Self {
        Array {
            shape: vec![],
            data: vec![value],
        }
    }

    pub fn flat(&self) -> &[T] {
        &self.data
    }

    fn size_of_shape(shape: &[usize]) -> usize {
        if shape.is_empty() {
            1
        } else {
            shape.iter().product()
        }
    }

    pub fn size(&self) -> usize {
        self.data.len()
    }
}

impl<T: Copy> Array<T> {
    pub fn fill(shape: &[usize], value: T) -> Self {
        let data = vec![value; Self::size_of_shape(shape)];
        Self {
            data,
            shape: shape.to_vec(),
        }
    }
}

impl<T: Zero + Copy> Array<T> {
    pub fn zeros(shape: &[usize]) -> Self {
        Self::fill(shape, T::zero())
    }
}

impl<T: One + Copy> Array<T> {
    pub fn ones(shape: &[usize]) -> Self {
        Self::fill(shape, T::one())
    }
}

/*
reduce cases:

[N] -> [1]
Vectorized application of op

[N, 2] -> [1, 2]

Loop over N and accumulate to first one.

Other

*/

impl<T: Copy> Array<T> {
    fn cwise_op<F: Fn(T, T) -> T>(f: F, a: &Array<T>, b: &Array<T>) -> Option<Array<T>> {
        let out_shape = match broadcast_shapes(&a.shape, &b.shape) {
            Some(v) => v,
            None => return None,
        };

        let out_size = Self::size_of_shape(&out_shape);

        let mut out_data = Vec::new();
        out_data.reserve(out_size);

        // NOTE: This implementation is optimized for operating on arrays that occupy
        // contiguous memory.

        if a.size() == 1 {
            // Case 1a: one of the arrays is a scalar.
            Self::cwise_op_scalar(f, b, a[0], &mut out_data);
        } else if b.size() == 1 {
            // Case 1b
            Self::cwise_op_scalar(f, a, b[0], &mut out_data);
        } else if a.size() == b.size() && a.size() == out_size {
            // Case 2: All same size.
            Self::cwise_op_vectorized(f, &a.data, &b.data, &mut out_data);
        } else {
            // Case 3: Generalized case.

            // TODO: Implement all other cases in terms of this (probably aside from the
            // scalar case). ^ Though some sub-executions may be a tensor *
            // scalar

            // TODO: Modify this case to exploit when some of the inner
            // dimensions are the same (this would be like applying
            // case 2 iteratively to build it out)

            let mut out_iter = ArrayIter::new(&out_shape);

            while let Some(out_idx) = out_iter.pos() {
                let a_idx = broadcasted_source_index(out_idx.flat(), &a.shape);
                let b_idx = broadcasted_source_index(out_idx.flat(), &b.shape);

                out_data.push(f(a[&a_idx[..]], b[&b_idx[..]]));

                out_iter.step();
            }
        }

        Some(Self {
            data: out_data,
            shape: out_shape,
        })
    }

    // Applies the cwise op assuming that 'a' and 'b' are the same shape.
    //
    // TODO: Support swapping in custom implementations of this per op (when we
    // think things can be vectorized using special CPU instructions).
    fn cwise_op_vectorized<F: Fn(T, T) -> T>(f: F, a: &[T], b: &[T], out_data: &mut Vec<T>) {
        for (a_i, b_i) in a.iter().zip(b.iter()) {
            out_data.push(f(*a_i, *b_i));
        }
    }

    fn cwise_op_scalar<F: Fn(T, T) -> T>(f: F, a: &Array<T>, b: T, out_data: &mut Vec<T>) {
        for a_i in a.data.iter().cloned() {
            out_data.push(f(a_i, b));
        }
    }

    /// Reduces the array using 'f' to combine values until all the given axes
    /// are reduced to a size of 1.
    ///
    /// NOTE: This does NOT remove axes. So the rank will be the same as the
    /// input.
    fn reduce_op<F: Fn(T, T) -> T>(
        f: F,
        input: &Array<T>,
        axes: &[isize],
        initial_value: T,
    ) -> Option<Array<T>> {
        let axes = axes
            .iter()
            .map(|a| input.resolve_axis(*a, false))
            .collect::<Vec<_>>();

        let mut output_shape = input.shape.clone();

        for d in &axes {
            if *d >= output_shape.len() {
                return None;
            }

            output_shape[*d] = 1;
        }

        let mut output = Self::fill(&output_shape, initial_value);

        let mut input_iter = input.iter();

        while let Some(input_idx) = input_iter.pos() {
            let mut output_idx = input_idx.clone();
            for d in &axes {
                output_idx[*d] = 0;
            }

            // TODO: Need to implement vectorization here when there is a long contiguous
            // span of bytes.
            output[&output_idx.data[..]] =
                f(output[&output_idx.data[..]], input[&input_idx.data[..]]);

            input_iter.step();
        }

        Some(output)
    }
}

////////////////////////////////////////////////////////////////////////////////
/// Add
////////////////////////////////////////////////////////////////////////////////

impl<T: Copy + Add<T, Output = T>> Array<T> {
    pub fn try_cwise_add(&self, other: &Array<T>) -> Option<Array<T>> {
        Self::cwise_op(|a, b| a + b, self, other)
    }
}

impl<T: Copy + PartialOrd<T>> Array<T> {
    pub fn cwise_max(&self, other: &Array<T>) -> Option<Array<T>> {
        Self::cwise_op(|a, b| if a > b { a } else { b }, self, other)
    }
}

impl<T: Copy + Add<T, Output = T> + Zero> Array<T> {
    pub fn sum(&self, axes: &[isize]) -> Array<T> {
        Self::reduce_op(|a, b| a + b, self, axes, T::zero()).unwrap()
    }
}

impl<T: Copy + Zero> Array<T> {
    /// TODO: Do nothing if already the correct shape.
    pub fn broadcast_to(&self, shape: &[usize]) -> Self {
        let out_shape = broadcast_shapes(&self.shape, shape).unwrap();
        // TODO: Require that any 1s in shape are also in out_shape?

        let mut out = Array::zeros(&out_shape);

        let mut out_iter = ArrayIter::new(&out_shape);
        while let Some(out_idx) = out_iter.pos() {
            let input_idx = broadcasted_source_index(out_idx.flat(), &self.shape);

            out[out_idx.flat()] = self[&input_idx[..]];

            out_iter.step();
        }

        out
    }
}

// Scalar addition
impl<T: Copy + AddAssign<T>> Add<T> for Array<T> {
    // + Add<T, Output=T>
    type Output = Array<T>;
    fn add(self, other: T) -> Array<T> {
        let mut data = self.data;
        for x in data.iter_mut() {
            *x += other;
        }

        Array::<T> {
            shape: self.shape,
            data,
        }
    }
}

// Array addition when all are the same shape
impl<T: AddAssign<T> + Copy> Add<Array<T>> for Array<T> {
    type Output = Array<T>;
    fn add(self, other: Array<T>) -> Array<T> {
        self.add(&other)
    }
}

impl<T: AddAssign<T> + Copy> Add<&Array<T>> for Array<T> {
    type Output = Array<T>;
    fn add(self, other: &Array<T>) -> Array<T> {
        assert!(self.shape == other.shape);
        let mut data = self.data;
        for (a, b) in data.iter_mut().zip(other.data.iter()) {
            *a += *b;
        }

        Array {
            data,
            shape: self.shape,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
/// Sub
////////////////////////////////////////////////////////////////////////////////

impl<T: SubAssign<T> + Copy> Sub<Array<T>> for Array<T> {
    type Output = Array<T>;
    fn sub(self, other: Array<T>) -> Array<T> {
        self.sub(&other)
    }
}

impl<T: SubAssign<T> + Copy> Sub<&Array<T>> for Array<T> {
    type Output = Array<T>;
    fn sub(self, other: &Array<T>) -> Array<T> {
        assert!(self.shape == other.shape);
        let mut data = self.data;
        for (a, b) in data.iter_mut().zip(other.data.iter()) {
            *a -= *b;
        }

        Array {
            data,
            shape: self.shape,
        }
    }
}

/*
impl<T: num_traits::CheckedSub> Array<T> {
    fn checked_sub(self, other: &Self) -> Option<Self> {
        assert!(self.shape == other.shape);
        let mut data = self.data;
        for (a, b) in data.iter_mut().zip(other.data.iter()) {
            *a = match a.checked_sub(b) {
                Some(x) => x,
                None => return None,
            };
        }

        Some(Array {
            data,
            shape: self.shape,
        })
    }
}
*/

impl<A: Copy> Array<A> {
    #[inline]
    pub fn map<F: Fn(A) -> B, B: Copy>(&self, f: F) -> Array<B> {
        let mut data = Vec::new();
        data.reserve(self.data.len());
        for a in self.data.iter() {
            data.push(f(*a));
        }

        Array {
            data,
            shape: self.shape.clone(),
        }
    }

    #[inline]
    pub fn map_into<F: Fn(A) -> A>(mut self, f: F) -> Array<A> {
        self.map_inplace(f);
        self
    }

    #[inline]
    pub fn map_inplace<F: Fn(A) -> A>(&mut self, f: F) {
        for x in self.data.iter_mut() {
            *x = f(*x);
        }
    }

    #[inline]
    pub fn zip<B: Copy, C, F: Fn(A, B) -> C>(&self, other: &Array<B>, f: F) -> Array<C> {
        assert_eq!(self.shape, other.shape);
        let mut data = Vec::new();
        data.reserve(self.data.len());
        for (a, b) in self.data.iter().zip(other.data.iter()) {
            data.push(f(*a, *b));
        }

        Array {
            data,
            shape: self.shape.clone(),
        }
    }

    #[inline]
    pub fn zip_into<B: Copy, F: Fn(A, B) -> A>(mut self, other: &Array<B>, f: F) -> Array<A> {
        self.zip_inplace(other, f);
        self
    }

    #[inline]
    pub fn zip_inplace<B: Copy, F: Fn(A, B) -> A>(&mut self, other: &Array<B>, f: F) {
        for (a, b) in self.data.iter_mut().zip(other.data.iter()) {
            *a = f(*a, *b);
        }
    }
}

/*
impl<T: num_traits::Float> Array<T> {
    pub fn sqrt(self) -> Array<T> { self.map(|x| x.sqrt()) }
    pub fn cos(self) -> Array<T> { self.map(|x| x.cos()) }
    pub fn sin(self) -> Array<T> { self.map(|x| x.sin()) }
    pub fn abs(self) -> Array<T> { self.map(|x| x.abs()) }
    pub fn atan2(self, other: &Array<T>) -> Array<T> { self.zip(other, |a, b| a.atan2(b)) }
}
impl<T: Copy + num_traits::Pow<T, Output=T>> Array<T> {
    pub fn pow(self, e: T) -> Array<T> { self.map(|x| x.pow(e)) }
}
*/

impl<T: Copy + Mul<T, Output = T>> Array<T> {
    pub fn cwise_mul(&self, other: &Self) -> Array<T> {
        Self::cwise_op(|a, b| a * b, self, other).unwrap()
    }
}

impl<T: Copy + Zero + Mul<T, Output = T> + AddAssign<T>> Array<T> {
    pub fn matmul(&self, other: &Self) -> Array<T> {
        // Check inner 2 most dimensions

        assert!(self.shape.len() >= 2);
        assert!(other.shape.len() >= 2);

        let (a_prefix, a_mat) = self.shape[..].split_at(self.shape.len() - 2);
        let (b_prefix, b_mat) = other.shape[..].split_at(other.shape.len() - 2);

        // Check compatibility of matrix inner dimensions.
        assert_eq!(a_mat[1], b_mat[0]);

        let out_prefix = broadcast_shapes(a_prefix, b_prefix).unwrap();

        let mut out_shape = out_prefix.clone();
        out_shape.push(a_mat[0]);
        out_shape.push(b_mat[1]);

        let mut out = Array::zeros(&out_shape);

        let mut out_prefix_iter = ArrayIter::new(&out_prefix);

        while let Some(out_prefix_idx) = out_prefix_iter.pos() {
            let a_prefix_idx = broadcasted_source_index(out_prefix_idx.flat(), &a_prefix);
            let b_prefix_idx = broadcasted_source_index(out_prefix_idx.flat(), &b_prefix);

            let mut out_idx = out_prefix_idx
                .flat()
                .iter()
                .map(|v| *v as usize)
                .collect::<Vec<_>>();
            // out_idx.extend_from_slice(&[0, 0]);

            let mut a_idx = a_prefix_idx.clone();
            // a_idx.extend_from_slice(&[0, 0]);

            let mut b_idx = b_prefix_idx.clone();
            // b_idx.extend_from_slice(&[0, 0]);

            for i in 0..a_mat[0] {
                for j in 0..b_mat[1] {
                    for k in 0..a_mat[1] {
                        a_idx.extend_from_slice(&[i, k]);
                        b_idx.extend_from_slice(&[k, j]);
                        out_idx.extend_from_slice(&[i, j]);

                        out[&out_idx[..]] += self[&a_idx[..]] * other[&b_idx[..]];

                        a_idx.pop();
                        a_idx.pop();

                        b_idx.pop();
                        b_idx.pop();

                        out_idx.pop();
                        out_idx.pop();
                    }
                }
            }

            out_prefix_iter.step();
        }

        out
    }
}

impl<T> Array<T> {
    fn resolve_axis(&self, i: isize, expanding: bool) -> usize {
        if i < 0 {
            if self.shape.len() == 0 {
                return 0;
            }

            let mut wrapped_i = self.shape.len() as isize + i;
            if expanding {
                wrapped_i += 1;
            }

            assert!(wrapped_i >= 0);
            wrapped_i as usize
        } else {
            i as usize
        }
    }
}

impl<T: Copy + Zero> Array<T> {
    pub fn swap_axes(&self, i: isize, j: isize) -> Array<T> {
        let i = self.resolve_axis(i, false);
        let j = self.resolve_axis(j, false);

        let mut new_shape = self.shape.clone();
        new_shape.swap(i, j);

        let mut out = Array::zeros(&new_shape);

        let mut out_iter = ArrayIter::new(&new_shape);

        while let Some(out_idx) = out_iter.pos() {
            let mut in_idx = out_idx.clone();
            in_idx.data.swap(i, j);

            out[out_idx.flat()] = self[in_idx.flat()];

            out_iter.step();
        }

        out
    }
}

impl<T: Copy + Div<T, Output = T>> Array<T> {
    pub fn cwise_div(&self, other: &Self) -> Array<T> {
        Self::cwise_op(|a, b| a / b, self, other).unwrap()
    }
}

impl<T: Copy + Mul<T, Output = T>> Mul<T> for Array<T> {
    type Output = Array<T>;
    fn mul(self, other: T) -> Array<T> {
        self.map(|x| x * other)
    }
}

impl<T: Copy + MulAssign<T>> Mul<Array<T>> for Array<T> {
    type Output = Array<T>;
    fn mul(self, other: Array<T>) -> Array<T> {
        assert_eq!(self.shape, other.shape);

        let mut data = self.data;
        for (a, b) in data.iter_mut().zip(other.data.iter()) {
            *a *= *b;
        }

        Array {
            data,
            shape: self.shape,
        }
    }
}

impl<T> core::convert::From<Vec<T>> for Array<T> {
    fn from(data: Vec<T>) -> Array<T> {
        let shape = vec![data.len()];
        Array { data, shape }
    }
}

impl<T: Clone> Array<T> {
    pub fn from_slice(data: &[T]) -> Array<T> {
        Array::from(data.iter().cloned().collect::<Vec<_>>())
    }
}

// TODO: Support a generic threshold?

impl<T: Clone> Array<T> {
    // Converts an index into a position tuple.
    fn to_pos<Y>(&self, mut idx: usize) -> Vec<Y>
    where
        usize: Cast<Y>,
    {
        let mut pos = Vec::<Y>::new();
        pos.reserve(self.shape.len());
        for i in (0..self.shape.len()).rev() {
            pos.push((idx % self.shape[i]).cast());
            idx /= self.shape[i];
        }

        pos.reverse();
        pos
    }

    fn from_pos<Y: Cast<usize> + Copy>(&self, pos: &[Y]) -> usize {
        assert_eq!(pos.len(), self.shape.len());
        let mut idx = 0;
        let mut inner_size: usize = self.shape.iter().product();
        for i in 0..pos.len() {
            inner_size /= self.shape[i];
            idx += pos[i].cast() * inner_size;
        }

        idx
    }

    pub fn contains_pos<S: core::cmp::PartialOrd<isize>>(&self, pos: &[S]) -> bool {
        for (i, p) in pos.iter().enumerate() {
            if *p < 0 || *p >= (self.shape[i] as isize) {
                return false;
            }
        }

        true
    }

    pub fn reshape(&self, new_shape: &[usize]) -> Array<T> {
        let new_size: usize = Self::size_of_shape(new_shape);
        assert_eq!(new_size, self.data.len());
        Array {
            shape: new_shape.iter().map(|x| *x).collect(),
            data: self.data.clone(),
        }
    }

    pub fn expand_dims(&self, axes: &[isize]) -> Array<T> {
        // TODO: Better match the numpy behavior.

        let mut normalized_axes = axes
            .iter()
            .map(|a| self.resolve_axis(*a, true))
            .collect::<Vec<_>>();
        normalized_axes.sort();

        let mut new_shape = self.shape.clone();
        for axis in normalized_axes.iter().rev() {
            new_shape.insert(*axis, 1);
        }

        self.reshape(&new_shape)
    }

    pub fn squeeze(&self, axes: &[isize]) -> Array<T> {
        let mut normalized_axes = axes
            .iter()
            .map(|a| self.resolve_axis(*a, false))
            .collect::<Vec<_>>();
        normalized_axes.sort();

        let mut new_shape = self.shape.clone();
        for axis in normalized_axes.iter().rev() {
            assert_eq!(new_shape[*axis], 1);
            new_shape.remove(*axis);
        }

        self.reshape(&new_shape)
    }

    pub fn iter(&self) -> ArrayIter {
        ArrayIter::new(&self.shape)
    }
}

pub struct ArrayIter<'a> {
    shape: &'a [usize], /* TODO: We can make this a slice in the Array, but that would make it
                         * hard to mutate the Array */
    cur_pos: Array<isize>, // Ideally we would return an iterator on an Array
    done: bool,
}

impl<'a> ArrayIter<'a> {
    fn new(shape: &'a [usize]) -> Self {
        Self {
            shape,
            cur_pos: Array::zeros(&[shape.len()]),
            done: false,
        }
    }

    pub fn pos(&'a self) -> Option<&'a Array<isize>> {
        if self.done {
            return None;
        }

        Some(&self.cur_pos)
    }

    pub fn step(&mut self) {
        for i in (0..self.shape.len()).rev() {
            self.cur_pos.data[i] += 1;
            // TODO: This assumes that all dims are > 0 (otherwise this will never
            // terminate)
            if self.cur_pos[i] == (self.shape[i] as isize) {
                self.cur_pos.data[i] = 0;
            } else {
                return;
            }
        }

        self.done = true
    }

    fn reset(&mut self) {
        self.done = false;
    }
}

// Defines what should be what
pub enum KernelEdgeMode {
    // Assume out of range pixels are 0
    Zero,
    // Take pixels from the other side of the kernel (or zero if those are also not in bounds).
    Mirror,
    // Use the value of the closest edge pixel.
    Extend,
}

impl<
        T: core::cmp::PartialOrd<T>
            + core::fmt::Display
            + Copy
            + Clone
            + core::ops::Mul<T, Output = T>
            + core::ops::AddAssign<T>
            + Zero,
    > Array<T>
{
    // Cross correlation. Currently just assumes that out of bound entries are zeros
    pub fn cross_correlate(&self, kernel: &Array<T>, edge_mode: KernelEdgeMode) -> Array<T> {
        assert_eq!(kernel.shape.len(), self.shape.len());
        for i in kernel.shape.iter() {
            assert!(i % 2 == 1); // Must all be odd to have a center.
        }

        let mut data = Vec::new();
        data.reserve(self.data.len());

        let kernel_center: Array<isize> = kernel
            .shape
            .iter()
            .map(|s| ((*s / 2) + 1) as isize)
            .collect::<Vec<_>>()
            .into();

        // Iterate over data indices.
        let mut self_iter = self.iter();
        let mut kernel_iter = kernel.iter();
        loop {
            {
                let self_pos = match self_iter.pos() {
                    Some(p) => p,
                    None => break,
                };
                let mut sum: T = T::zero();

                // Iterate over kernel indices
                kernel_iter.reset();
                for kernel_idx in 0..kernel.data.len() {
                    {
                        // Position tuple in the kernel
                        let kernel_pos = match kernel_iter.pos() {
                            Some(p) => p,
                            None => break,
                        };

                        // Position tuple in the kernel relative to the center of the
                        let kernel_cpos = kernel_pos.clone() - &kernel_center;

                        // Position tuple in the array
                        let p = self_pos.clone() + &kernel_cpos;

                        let val = if self.contains_pos(&p.data) {
                            self[&p.data[..]]
                        } else {
                            match edge_mode {
                                KernelEdgeMode::Zero => T::zero(),
                                // TODO: For these implement the case that they are also out of
                                // bounds.
                                KernelEdgeMode::Mirror => {
                                    // For any dimension outside of the array, flip it around the
                                    // center of the kernel.
                                    let mut new_kernel_cpos = kernel_cpos;
                                    for i in 0..p.data.len() {
                                        if p.data[i] < 0 || p.data[i] >= (self.shape[i] as isize) {
                                            new_kernel_cpos.data[i] *= -1;
                                        }
                                    }

                                    // Recalculate position using new kernel centered position.
                                    let new_p = new_kernel_cpos + self_pos;
                                    if self.contains_pos(&new_p.data) {
                                        self[&new_p.data[..]]
                                    } else {
                                        T::zero()
                                    }
                                }
                                KernelEdgeMode::Extend => {
                                    // For each out of bounds dimension, clip it to the nearest
                                    // inbounds position.
                                    let mut new_p = p;
                                    for i in 0..new_p.data.len() {
                                        if new_p.data[i] < 0 {
                                            new_p.data[i] = 0;
                                        } else if new_p.data[i] >= (self.shape[i] as isize) {
                                            new_p.data[i] = (self.shape[i] - 1) as isize;
                                        }
                                    }

                                    self[&new_p.data[..]]
                                }
                            }
                        };

                        sum += val * kernel[kernel_idx];
                    }
                    kernel_iter.step();
                }

                data.push(sum);
            }
            self_iter.step();
        }

        Array {
            shape: self.shape.clone(),
            data,
        }
    }
}

impl<T: Copy + Default> Array<T> {
    // Flips the array along one dimension.
    pub fn flip(&self, dim: usize) -> Array<T> {
        assert!(dim < self.shape.len());

        let mut data = Vec::<T>::new();
        data.resize(self.data.len(), T::default());
        for idx_old in 0..self.data.len() {
            let mut pos = self.to_pos::<usize>(idx_old);
            pos[dim] = self.shape[dim] - pos[dim] - 1;
            let idx_new = self.from_pos(&pos);
            data[idx_new] = self.data[idx_old];
        }

        Array::<T> {
            data,
            shape: self.shape.clone(),
        }
    }

    /*
    pub fn threshold(&mut self, val: usize) {
        self.data =
    }
    */

    // TODO: Allow zero copy reshape

    /*
    // Extracts a
    // TODO: Need to support a zero copy version of this that reads a view of the image (this will be useful for implementing cross correlation).
    pub fn slice(&self, index: &[ArrayIndex]) -> Array<T> {
        assert_eq!(index.len(), self.shape.len());
        let parts = index.iter().zip(0..index.len()).map(|(idx, i)| match idx {
            ArrayIndex::All => (0, self.shape[i]),
            // TODO: Bounds check these?
            ArrayIndex::One(v) => (*v, *v + 1),
            ArrayIndex::Range(s, e) => (*s, *e) // Assert e > s (not equal)
        }).collect::<Vec<_>>();

        let size = parts.iter().fold(1, |prod, (s, e)| prod*(e - s));
        let data = Vec::new();
        data.reserve(size);

        // TODO: F
    }
    */
}

// Converting between data types.
impl<T> Array<T> {
    pub fn cast<Y: Copy>(&self) -> Array<Y>
    where
        Y: 'static,
        T: Cast<Y> + Copy,
    {
        Array::<Y> {
            shape: self.shape.clone(),
            data: self.data.iter().map(|x| (*x).cast()).collect::<Vec<Y>>(),
        }
    }
}

impl<T: Clone> core::ops::Index<usize> for Array<T> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

// Index single entry in array by position tuple.
impl<T: Clone, Y: Cast<usize> + Copy> core::ops::Index<&[Y]> for Array<T> {
    type Output = T;
    fn index(&self, index: &[Y]) -> &Self::Output {
        &self.data[self.from_pos(index)]
    }
}

impl<T: Clone> core::ops::IndexMut<usize> for Array<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
        &mut self.data[index]
    }
}
impl<T: Clone, Y: Cast<usize> + Copy> core::ops::IndexMut<&[Y]> for Array<T> {
    fn index_mut(&mut self, index: &[Y]) -> &mut T {
        let idx = self.from_pos(index);
        &mut self.data[idx]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_dims_scalar_test() {
        let arr = Array::<f32>::zeros(&[]);

        let arr2 = arr.expand_dims(&[0]);
        assert_eq!(&arr2.shape[..], &[1]);

        let arr2 = arr.expand_dims(&[-1]);
        assert_eq!(&arr2.shape[..], &[1]);
    }

    #[test]
    fn expand_dims_1d_test() {
        let arr = Array::<f32>::zeros(&[4]);

        let arr2 = arr.expand_dims(&[0]);
        assert_eq!(&arr2.shape[..], &[1, 4]);

        let arr2 = arr.expand_dims(&[-1]);
        assert_eq!(&arr2.shape[..], &[4, 1]);
    }
}
