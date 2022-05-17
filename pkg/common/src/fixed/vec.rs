use core::iter::Iterator;
use core::marker::PhantomData;
use core::mem::zeroed;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};

use crate::const_default::ConstDefault;

// TODO: We can implement IntoIter with a swap with the last element and then
// popping the last element

/// Fixed capacity vector which can store 0-LEN copies of T.
pub struct FixedVec<T, const LEN: usize> {
    data: [MaybeUninit<T>; LEN],
    length: usize,
}

impl<T, const LEN: usize> FixedVec<T, LEN> {
    pub const fn new() -> Self {
        Self {
            data: MaybeUninit::uninit_array(),
            length: 0,
        }
    }

    pub fn push(&mut self, value: T) {
        self.data[self.length] = MaybeUninit::new(value);
        self.length += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.length == 0 {
            return None;
        }

        let mut value = MaybeUninit::uninit();
        self.length -= 1;
        core::mem::swap(&mut value, &mut self.data[self.length]);

        Some(unsafe { value.assume_init() })
    }

    pub fn remove(&mut self, mut index: usize) {
        assert!(index < self.length);

        // Bubble the element to be removed to the end of the list.
        while index < self.length - 1 {
            self.data.as_mut().swap(index, index + 1);
            index += 1;
        }

        self.pop();
    }

    pub fn clear(&mut self) {
        while self.length > 0 {
            self.pop();
        }
    }

    pub fn truncate(&mut self, new_size: usize) {
        assert!(new_size <= self.length);
        while self.length > new_size {
            self.pop();
        }
    }

    pub fn iter(&self) -> Iter<T, LEN> {
        Iter {
            inst: self,
            index: 0,
        }
    }

    pub fn into_iter(self) -> IntoIter<T, LEN> {
        IntoIter {
            inst: self,
            index: 0,
        }
    }

    pub fn resize(&mut self, new_size: usize, value: T)
    where
        T: Clone,
    {
        if new_size < self.length {
            return self.truncate(new_size);
        }

        while self.length < new_size {
            self.push(value.clone());
        }
    }
}

impl<T, const LEN: usize> Drop for FixedVec<T, LEN> {
    fn drop(&mut self) {
        for i in 0..self.length {
            unsafe { self.data[i].assume_init_drop() };
        }
    }
}

// impl<T, const LEN: usize> Iterator for FixedVec<T, LEN> {}

impl<T: Clone, const LEN: usize> From<&[T]> for FixedVec<T, LEN> {
    fn from(v: &[T]) -> Self {
        // TODO: Optimize for &[u8]?
        let mut inst = Self::new();
        for v in v {
            inst.push(v.clone());
        }

        inst
    }
}

impl<T, const LEN: usize> Default for FixedVec<T, LEN> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const LEN: usize> ConstDefault for FixedVec<T, LEN> {
    const DEFAULT: Self = Self::new();
}

impl<T, const LEN: usize> Deref for FixedVec<T, LEN> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        unsafe { MaybeUninit::slice_assume_init_ref(&self.data[0..self.length]) }
    }
}

impl<T, const LEN: usize> DerefMut for FixedVec<T, LEN> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { MaybeUninit::slice_assume_init_mut(&mut self.data[0..self.length]) }
    }
}

impl<T: Clone, const LEN: usize> Clone for FixedVec<T, LEN> {
    fn clone(&self) -> Self {
        let mut out = Self::new();
        for v in Deref::deref(self) {
            out.push(v.clone());
        }

        out
    }
}

impl<T: PartialEq, const LEN: usize> PartialEq for FixedVec<T, LEN> {
    fn eq(&self, other: &Self) -> bool {
        // Use slice comparison.
        *self == *other
    }
}

pub struct Iter<'a, T, const LEN: usize> {
    inst: &'a FixedVec<T, LEN>,
    index: usize,
}

impl<'a, T, const LEN: usize> Iterator for Iter<'a, T, LEN> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.inst.len() {
            let ret = Some(&self.inst[self.index]);
            self.index += 1;
            ret
        } else {
            None
        }
    }
}

/// Internally implemented by incrementally reversing the vector on each next()
/// call. This allows us to remove the next forward order element by just using
/// pop().
pub struct IntoIter<T, const LEN: usize> {
    inst: FixedVec<T, LEN>,
    index: usize,
}

impl<T, const LEN: usize> Iterator for IntoIter<T, LEN> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.inst.len() && self.index != self.inst.len() - 1 {
            let i = self.index;
            let j = self.inst.length - 1;

            self.inst.data.swap(i, j);
            self.index += 1;
        }

        self.inst.pop()
    }
}
