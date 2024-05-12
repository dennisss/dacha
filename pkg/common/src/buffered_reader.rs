use alloc::boxed::Box;
use alloc::vec::Vec;

use base_error::*;

use crate::io::Readable;

const BUFFER_SIZE: usize = 16 * 1024;

/// A reader which always an inner reader with a large buffer size and buffers
/// the data until the user has read all of it.
pub struct BufferedReader<R> {
    inner: R,
    buffer: Vec<u8>,
    buffer_offset: usize,
    buffer_length: usize,
}

impl<R> BufferedReader<R> {
    pub fn new(inner: R) -> Self {
        let mut buffer = vec![0u8; BUFFER_SIZE];
        Self {
            inner,
            buffer,
            buffer_offset: 0,
            buffer_length: 0,
        }
    }
}

#[async_trait]
impl<R: Readable> Readable for BufferedReader<R> {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        // TOOD: If 'buf' is big and self.buffer is empty, then we don't need to do any
        // buffering.

        if self.buffer_offset == self.buffer_length {
            let n = self.inner.read(&mut self.buffer).await?;
            self.buffer_offset = 0;
            self.buffer_length = n;
        }

        let n = core::cmp::min(self.buffer_length - self.buffer_offset, buf.len());
        buf[0..n].copy_from_slice(&self.buffer[self.buffer_offset..(self.buffer_offset + n)]);
        self.buffer_offset += n;
        Ok(n)
    }
}
