use core::convert::Infallible;
use core::ops::Try;
use core::result::Result;

use generic_array::ArrayLength;

use crate::errors::error_new::IntoError;
use crate::fixed::vec::FixedVec;

pub trait List<T>: Appendable<Item = T> + Clearable {}

#[cfg(feature = "alloc")]
impl<T: Clone> List<T> for alloc::vec::Vec<T> {}

impl<T: Clone, const LEN: usize> List<T> for FixedVec<T, LEN> {}

pub trait Appendable {
    type Item;
    type Error: IntoError + Send + Sync + 'static;

    fn push(&mut self, value: Self::Item) -> Result<(), Self::Error>;

    fn extend_from_slice(&mut self, other: &[Self::Item]) -> Result<(), Self::Error>;
}

#[cfg(feature = "alloc")]
impl<T: Clone> Appendable for alloc::vec::Vec<T> {
    type Item = T;
    type Error = Infallible;

    fn push(&mut self, value: Self::Item) -> Result<(), Self::Error> {
        alloc::vec::Vec::push(self, value);
        Ok(())
    }

    fn extend_from_slice(&mut self, other: &[Self::Item]) -> Result<(), Self::Error> {
        alloc::vec::Vec::extend_from_slice(self, other);
        Ok(())
    }
}

impl<T: Clone, const LEN: usize> Appendable for FixedVec<T, LEN> {
    type Item = T;
    // TODO: Return an error instead of panicking when there is overflow
    type Error = Infallible;

    fn push(&mut self, value: Self::Item) -> Result<(), Self::Error> {
        FixedVec::push(self, value);
        Ok(())
    }

    fn extend_from_slice(&mut self, other: &[Self::Item]) -> Result<(), Self::Error> {
        for item in other {
            self.push(item.clone());
        }

        Ok(())
    }
}

pub trait Clearable {
    fn clear(&mut self);
}

#[cfg(feature = "alloc")]
impl<T> Clearable for alloc::vec::Vec<T> {
    fn clear(&mut self) {
        alloc::vec::Vec::clear(self);
    }
}

impl<T, const LEN: usize> Clearable for FixedVec<T, LEN> {
    fn clear(&mut self) {
        FixedVec::clear(self);
    }
}

pub struct ByteCounter {
    total: usize,
}

impl ByteCounter {
    pub fn new() -> Self {
        Self { total: 0 }
    }

    pub fn total_bytes(&self) -> usize {
        self.total
    }
}

impl Appendable for ByteCounter {
    type Item = u8;
    type Error = Infallible;

    fn push(&mut self, value: Self::Item) -> Result<(), Self::Error> {
        self.total += 1;
        Ok(())
    }

    fn extend_from_slice(&mut self, other: &[Self::Item]) -> Result<(), Self::Error> {
        self.total += other.len();
        Ok(())
    }
}
