use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

use crate::const_default::ConstDefault;

#[derive(Default, PartialEq, Clone)]
pub struct FixedVec<T, A: AsRef<[T]> + AsMut<[T]>> {
    data: A,
    length: usize,
    ty: PhantomData<T>,
}

impl<T: Default, A: AsRef<[T]> + AsMut<[T]>> FixedVec<T, A> {
    pub const fn new(data: A) -> Self {
        Self {
            data,
            length: 0,
            ty: PhantomData,
        }
    }

    pub fn push(&mut self, value: T) {
        self.data.as_mut()[self.length] = value;
        self.length += 1;
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.length == 0 {
            return None;
        }

        let mut value = T::default();
        self.length -= 1;
        core::mem::swap(&mut value, &mut self.data.as_mut()[self.length]);

        Some(value)
    }

    pub fn clear(&mut self) {
        while self.length > 0 {
            self.pop();
        }
    }
}

impl<T: Default, A: AsRef<[T]> + AsMut<[T]> + ConstDefault> ConstDefault for FixedVec<T, A> {
    const DEFAULT: Self = Self::new(A::DEFAULT);
}

impl<T: Default, A: AsRef<[T]> + AsMut<[T]>> Deref for FixedVec<T, A> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.data.as_ref()
    }
}

impl<T: Default, A: AsRef<[T]> + AsMut<[T]>> DerefMut for FixedVec<T, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data.as_mut()
    }
}
