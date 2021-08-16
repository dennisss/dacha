/// Enables reading from
pub struct SliceReader<'a> {
    slices: [&'a [u8]; 2],
    index: usize,
    consumed_bytes: usize,
}

impl<'a> SliceReader<'a> {
    pub fn new(slices: [&'a [u8]; 2]) -> Self {
        Self {
            slices,
            index: 0,
            consumed_bytes: 0,
        }
    }

    pub fn reserve(&self, n: usize) -> std::io::Result<()> {
        let mut len = 0;
        for s in &self.slices {
            len += s.len();
        }

        if len < n {
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, ""));
        }

        Ok(())
    }

    pub fn consumed_bytes(&self) -> usize {
        self.consumed_bytes
    }
}

impl<'a> std::io::Read for SliceReader<'a> {
    fn read(&mut self, mut output: &mut [u8]) -> std::io::Result<usize> {
        let mut total_read = 0;
        while output.len() > 0 && self.index < self.slices.len() {
            if self.slices[self.index].is_empty() {
                self.index += 1;
                continue;
            }

            let n = std::cmp::min(self.slices[self.index].len(), output.len());
            let (slice_head, slice_rest) = self.slices[self.index].split_at(n);
            let (output_head, output_rest) = output.split_at_mut(n);

            output_head.copy_from_slice(slice_head);
            total_read += n;

            self.slices[self.index] = slice_rest;
            output = output_rest;
        }

        self.consumed_bytes += total_read;
        Ok(total_read)
    }
}
