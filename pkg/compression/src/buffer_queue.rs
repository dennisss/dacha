
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
        Self { buffer: vec![], buffer_offset: 0 }
    }

    /// Copies from the internal output buffer into the provided buffer.
    /// Returns the number of bytes that were copied.
    pub fn copy_to(&mut self, output: &mut [u8]) -> usize {
        // Number of bytes remaining in the internal buffer.
        let rem = self.buffer.len() - self.buffer_offset;

        let n = std::cmp::min(rem, output.len());
        output[0..n].copy_from_slice(
            &self.buffer[self.buffer_offset..(self.buffer_offset + n)],
        );

        self.buffer_offset += n;
        if self.buffer_offset == self.buffer.len() {
            self.buffer.clear();
            self.buffer_offset = 0;
        }

        n
    }

    pub fn is_empty(&self) -> bool {
        // NOTE: self.buffer_offset == self.buffer.len() will only happen with
        // self.buffer_offset == 0 because we clear the buffer when we get to
        // the end of it.
        self.buffer.is_empty()
    }
}