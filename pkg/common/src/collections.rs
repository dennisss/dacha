use core::ops::{Deref, DerefMut};

use generic_array::{ArrayLength, GenericArray};

pub struct FixedVec<T, N: ArrayLength<T>> {
    data: GenericArray<T, N>,
    length: usize,
}

impl<T: Default, N: ArrayLength<T>> FixedVec<T, N> {
    pub fn new() -> Self {
        Self {
            data: GenericArray::default(),
            length: 0,
        }
    }

    pub fn push(&mut self, value: T) {
        self.data[self.length] = value;
        self.length += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.length == 0 {
            return None;
        }

        let mut value = T::default();
        self.length -= 1;
        core::mem::swap(&mut value, &mut self.data[self.length]);

        Some(value)
    }
}

impl<T: Default, N: ArrayLength<T>> Deref for FixedVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T: Default, N: ArrayLength<T>> DerefMut for FixedVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}
