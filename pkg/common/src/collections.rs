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

    pub fn truncate(&mut self, new_size: usize) {
        assert!(new_size <= self.length);
        while self.length > new_size {
            self.pop();
        }
    }
}

impl<T: Default + Clone, A: AsRef<[T]> + AsMut<[T]>> FixedVec<T, A> {
    pub fn resize(&mut self, new_size: usize, value: T) {
        if new_size < self.length {
            return self.truncate(new_size);
        }

        while self.length < new_size {
            self.push(value.clone());
        }
    }
}

impl<T: Default + Clone, A: AsRef<[T]> + AsMut<[T]> + Default> From<&[T]> for FixedVec<T, A> {
    fn from(v: &[T]) -> Self {
        // TODO: Optimize for &[u8]?
        let mut inst = Self::new(A::default());
        for v in v {
            inst.push(v.clone());
        }

        inst
    }
}

impl<T: Default, A: AsRef<[T]> + AsMut<[T]> + ConstDefault> ConstDefault for FixedVec<T, A> {
    const DEFAULT: Self = Self::new(A::DEFAULT);
}

impl<T: Default, A: AsRef<[T]> + AsMut<[T]>> Deref for FixedVec<T, A> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.data.as_ref()[0..self.length]
    }
}

impl<T: Default, A: AsRef<[T]> + AsMut<[T]>> DerefMut for FixedVec<T, A> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data.as_mut()[0..self.length]
    }
}

#[derive(Default, Clone, PartialEq)]
pub struct FixedString<A> {
    data: A,
    length: usize,
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> FixedString<A> {
    pub const fn new(data: A) -> Self {
        Self { data, length: 0 }
    }

    pub fn push(&mut self, c: char) {
        let remaining = &mut self.data.as_mut()[self.length..];
        self.length += c.encode_utf8(remaining).len();
    }

    pub fn push_str(&mut self, s: &str) {
        let remaining = &mut self.data.as_mut()[self.length..];
        remaining[0..s.len()].copy_from_slice(s.as_bytes());
        self.length += s.len();
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]> + Default> From<&str> for FixedString<A> {
    fn from(v: &str) -> Self {
        let mut inst = Self::new(A::default());
        inst.push_str(v);
        inst
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> AsRef<[u8]> for FixedString<A> {
    fn as_ref(&self) -> &[u8] {
        &self.data.as_ref()[0..self.length]
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> AsRef<str> for FixedString<A> {
    fn as_ref(&self) -> &str {
        // All operations we implement are valid UTF-8 mutations so the underlying
        // storage should always contain valid UTF-8 data.
        unsafe { core::str::from_utf8_unchecked(self.data.as_ref()) }
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]>> Deref for FixedString<A> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<A: AsRef<[u8]> + AsMut<[u8]> + ConstDefault> ConstDefault for FixedString<A> {
    const DEFAULT: Self = Self::new(A::DEFAULT);
}
