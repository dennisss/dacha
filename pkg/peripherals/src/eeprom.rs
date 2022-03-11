use core::future::Future;

use common::errors::*;

pub trait EEPROM {
    type ReadFuture<'a>: Future<Output = Result<()>> + 'a
    where
        Self: 'a;

    type WriteFuture<'a>: Future<Output = Result<()>> + 'a
    where
        Self: 'a;

    fn total_size(&self) -> usize;

    /// Returns the size of a single page in the EEPROM. This would be the
    /// largest/smallest amount of data that can be written in one operation.
    fn page_size(&self) -> usize;

    fn read<'a>(&'a mut self, offset: usize, data: &'a mut [u8]) -> Self::ReadFuture<'a>;

    fn write<'a>(&'a mut self, offset: usize, data: &'a [u8]) -> Self::WriteFuture<'a>;
}
