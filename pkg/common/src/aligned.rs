use core::alloc::Allocator;
use core::ops::{Deref, DerefMut};

use alloc::alloc::{Global, Layout};
use alloc::vec::Vec;

/// Vec which maintains a user specified address alignment for the start offset
/// of its data.
///
/// Internally this relies on replacing all of Vec's calls to alloc() with new
/// calls that respect the desired alignment.
pub struct AlignedVec<T> {
    data: Vec<T>,
}

impl<T: Default + Copy> AlignedVec<T> {
    pub fn new(len: usize, alignment: usize) -> Self {
        let layout = Layout::array::<T>(len)
            .unwrap()
            .align_to(alignment)
            .unwrap();

        let mut data = unsafe {
            let ptr = Global.allocate(layout).unwrap();
            Vec::<T>::from_raw_parts(ptr.cast::<T>().as_ptr(), 0, len)
        };

        data.resize(len, T::default());

        Self { data }
    }
}

impl<T> Deref for AlignedVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for AlignedVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}
