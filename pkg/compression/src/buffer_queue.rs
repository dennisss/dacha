use std::io::Read;

use common::errors::Result;

use crate::slice_reader::SliceReader;

/// Byte queue designed for
pub struct BufferQueue {
    // TODO: If this ever needs to be resized, we should prefer to skip copying any consumed bytes
    pub buffer: Vec<u8>,

    /// Offset into buffer representing how many bytes have been consumed
    /// by the user
    buffer_offset: usize,
}

impl BufferQueue {
    pub fn new() -> Self {
        Self {
            buffer: vec![],
            buffer_offset: 0,
        }
    }

    /// Copies from the internal output buffer into the provided buffer.
    /// Returns the number of bytes that were copied.
    pub fn copy_to(&mut self, output: &mut [u8]) -> usize {
        // Number of bytes remaining in the internal buffer.
        let rem = self.buffer.len() - self.buffer_offset;

        let n = std::cmp::min(rem, output.len());
        output[0..n].copy_from_slice(&self.buffer[self.buffer_offset..(self.buffer_offset + n)]);

        self.buffer_offset += n;
        if self.buffer_offset == self.buffer.len() {
            self.buffer.clear();
            self.buffer_offset = 0;
        }

        n
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.buffer_offset = 0;
    }

    pub fn is_empty(&self) -> bool {
        // NOTE: self.buffer_offset == self.buffer.len() will only happen with
        // self.buffer_offset == 0 because we clear the buffer when we get to
        // the end of it.
        self.buffer.is_empty()
    }

    pub fn try_read<T>(
        &mut self,
        input: &[u8],
        f: fn(&mut dyn Read) -> Result<T>,
    ) -> Result<(Option<T>, usize)> {
        let mut reader = SliceReader::new([&self.buffer, input]);
        let result = f(&mut reader);

        let mut consumed_bytes = reader.consumed_bytes();
        consumed_bytes -= self.buffer.len();

        let value = match result {
            Ok(v) => v,
            Err(e) => {
                if let Some(io_error) = e.downcast_ref::<std::io::Error>() {
                    if io_error.kind() == std::io::ErrorKind::UnexpectedEof {
                        // NOTE: In this case, consumed_bytes should always be equal to input.len().
                        self.buffer.extend_from_slice(&input[0..consumed_bytes]);
                        return Ok((None, consumed_bytes));
                    }
                }

                return Err(e);
            }
        };

        self.clear();

        Ok((Some(value), consumed_bytes))
    }
}
