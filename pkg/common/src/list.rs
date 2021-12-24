use core::convert::Infallible;
use core::ops::Try;
use core::result::Result;

use generic_array::ArrayLength;

use crate::collections::FixedVec;
use crate::errors::error_new::IntoError;

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

impl<T: Default + Clone, A: AsRef<[T]> + AsMut<[T]>> Appendable for FixedVec<T, A> {
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
