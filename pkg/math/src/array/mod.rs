use std::ops::{Add, Mul, Div, Sub, AddAssign, SubAssign};
use num_traits::{Num, NumCast, AsPrimitive};

// TODO: First implement an ArrayMut which has operations that are allowed to mutate the vector in it.
// Then implement regular Array as a Ref counted ArrayMut

// Ideally 

// N-dimensional array of data with known type.
#[derive(Clone)]
pub struct Array<T> {
	pub shape: Vec<usize>,
	pub data: Vec<T>

	// TODO: Could use general scalar multiplication, addition, division
}

// impl<T: Copy> Copy for Array<T> where T: Copy {}

pub enum ArrayIndex {
	All,
	One(usize),
	Range(usize, usize)
}

// Scalar addition
impl<T: Copy + AddAssign<T>> Add<T> for Array<T> { // + Add<T, Output=T>
	type Output = Array<T>;
	fn add(self, other: T) -> Array<T> {
		let mut data = self.data;
		for x in data.iter_mut() {
			*x += other;
		}

		Array::<T> {
			shape: self.shape,
			data
		}
	}
}

// Array addition when all are the same shape
impl<T: AddAssign<T> + Copy> Add<Array<T>> for Array<T> {
	type Output = Array<T>;
	fn add(self, other: Array<T>) -> Array<T> { self.add(&other) }
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
			data, shape: self.shape
		}
	}
}

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
			data, shape: self.shape
		}
	}
}

impl<T: num_traits::CheckedSub> Array<T> {
	fn checked_sub(self, other: &Self) -> Option<Self> {
		assert!(self.shape == other.shape);
		let mut data = self.data;
		for (a, b) in data.iter_mut().zip(other.data.iter()) {
			*a = match a.checked_sub(b) {
				Some(x) => x,
				None => return None
			};
		}

		Some(Array {
			data, shape: self.shape
		})
	}
}

impl<A: Copy> Array<A> {
	#[inline]
	pub fn map<F: Fn(A) -> B, B: Copy>(&self, f: F) -> Array<B> {
		let mut data = Vec::new();
		data.reserve(self.data.len());
		for a in self.data.iter() {
			data.push(f(*a));
		}

		Array { data, shape: self.shape.clone() }
	}

	#[inline]
	pub fn map_into<F: Fn(A) -> A>(mut self, f: F) -> Array<A> {
		self.map_inplace(f); self
	}

	#[inline]
	pub fn map_inplace<F: Fn(A) -> A>(&mut self, f: F) {
		for x in self.data.iter_mut() { *x = f(*x); }
	}

	#[inline]
	pub fn zip<B: Copy, C, F: Fn(A, B) -> C>(&self, other: &Array<B>, f: F) -> Array<C> {
		assert_eq!(self.shape, other.shape);
		let mut data = Vec::new();
		data.reserve(self.data.len());
		for (a, b) in self.data.iter().zip(other.data.iter()) {
			data.push(f(*a, *b));
		}

		Array { data, shape: self.shape.clone() }
	}

	#[inline]
	pub fn zip_into<B: Copy, F: Fn(A, B) -> A>(mut self, other: &Array<B>, f: F) -> Array<A> {
		self.zip_inplace(other, f); self
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

impl<T: Copy + Mul<T, Output=T>> Mul<T> for Array<T> {
	type Output = Array<T>;
	fn mul(self, other: T) -> Array<T> {
		self.map(|x| x*other)
	}
}

impl<T: Clone> std::convert::From<Vec<T>> for Array<T> {
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
	fn to_pos<Y: NumCast>(&self, mut idx: usize) -> Vec<Y> {
		let mut pos = Vec::<Y>::new();
		pos.reserve(self.shape.len());
		for i in (0..self.shape.len()).rev() {
			pos.push(Y::from(idx % self.shape[i]).unwrap());
			idx /= self.shape[i];
		}

		pos.reverse();
		pos
	}

	fn from_pos<Y: AsPrimitive<usize>>(&self, pos: &[Y]) -> usize {
		assert_eq!(pos.len(), self.shape.len());
		let mut idx = 0;
		let mut inner_size: usize = self.shape.iter().product();
		for i in 0..pos.len() {
			inner_size /= self.shape[i];
			idx +=  pos[i].as_()*inner_size;
		}

		idx
	}

	pub fn contains_pos<S: std::cmp::PartialOrd<isize>>(&self, pos: &[S]) -> bool {
		for (i, p) in pos.iter().enumerate() {
			if *p < 0 || *p >= (self.shape[i] as isize) {
				return false;
			}
		}

		true
	}

	pub fn reshape(&self, new_shape: &[usize]) -> Array<T> {
		let new_size: usize = new_shape.iter().product();
		assert_eq!(new_size, self.data.len());
		Array {
			shape: new_shape.iter().map(|x| *x).collect(),
			data: self.data.clone()
		}
	}

	pub fn iter(&self) -> ArrayIter {
		ArrayIter {
			shape: &self.shape,
			cur_pos: Array::from(vec![0,0]), // TODO: Must be dynamic to the shape
			done: false
		}
	}
}

pub struct ArrayIter<'a> {
	shape: &'a [usize], // TODO: We can make this a slice in the Array, but that would make it hard to mutate the Array
	cur_pos: Array<isize>, // Ideally we would return an iterator on an Array
	done: bool
}

impl<'a> ArrayIter<'a> {
	pub fn pos(&'a self) -> Option<&'a Array<isize>> {
		if self.done {
			return None;
		}

		Some(&self.cur_pos)
	}

	pub fn step(&mut self) {
		for i in (0..self.shape.len()).rev() {
			self.cur_pos.data[i] += 1;
			// TODO: This assumes that all dims are > 0 (otherwise this will never terminate)
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
	Extend
}

impl<T: std::cmp::PartialOrd<T> + std::fmt::Display + Copy + Clone + std::ops::Mul<T, Output=T> + std::ops::AddAssign<T> + num_traits::Zero> Array<T> {

	// Cross correlation. Currently just assumes that out of bound entries are zeros
	pub fn cross_correlate(&self, kernel: &Array<T>, edge_mode: KernelEdgeMode) -> Array<T> {
		assert_eq!(kernel.shape.len(), self.shape.len());
		for i in kernel.shape.iter() {
			assert!(i % 2 == 1); // Must all be odd to have a center.
		}

		let mut data = Vec::new();
		data.reserve(self.data.len());

		let kernel_center: Array<isize> = kernel.shape.iter().map(|s| ((*s / 2) + 1) as isize).collect::<Vec<_>>().into();

		// Iterate over data indices.
		let mut self_iter = self.iter();
		let mut kernel_iter = kernel.iter();
		loop {
			{
			let self_pos = match self_iter.pos() {
				Some(p) => p,
				None => break 
			};
			let mut sum: T = T::zero();

			// Iterate over kernel indices
			kernel_iter.reset();
			for kernel_idx in 0..kernel.data.len() {
				{
				// Position tuple in the kernel
				let kernel_pos = match kernel_iter.pos() {
					Some(p) => p,
					None => break
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
						// TODO: For these implement the case that they are also out of bounds.
						KernelEdgeMode::Mirror => {
							// For any dimension outside of the array, flip it around the center of the kernel.
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
						},
						KernelEdgeMode::Extend => {
							// For each out of bounds dimension, clip it to the nearest inbounds position.
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
			data
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

		Array::<T> { data, shape: self.shape.clone() }
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
	pub fn cast<Y: Copy>(&self) -> Array<Y> where Y: 'static, T: AsPrimitive<Y> {
		Array::<Y> {
			shape: self.shape.clone(),
			data: self.data.iter().map(|x| (*x).as_()).collect::<Vec<Y>>()
		}
	}
}

impl<T: Clone> std::ops::Index<usize> for Array<T> {
	type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
		&self.data[index]
	}
}

// Index single entry in array by position tuple.
impl<T: Clone, Y: AsPrimitive<usize>> std::ops::Index<&[Y]> for Array<T> {
	type Output = T;
    fn index(&self, index: &[Y]) -> &Self::Output {
		&self.data[self.from_pos(index)]
	}
}

impl<T: Clone> std::ops::IndexMut<usize> for Array<T> {
    fn index_mut(&mut self, index: usize) -> &mut T {
		&mut self.data[index]
	}
}
impl<T: Clone, Y: AsPrimitive<usize>> std::ops::IndexMut<&[Y]> for Array<T> {
    fn index_mut(&mut self, index: &[Y]) -> &mut T {
		let idx = self.from_pos(index);
		&mut self.data[idx]
	}
}
