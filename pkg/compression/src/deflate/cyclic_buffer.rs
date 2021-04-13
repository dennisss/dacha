pub trait WindowBuffer: std::ops::Index<usize, Output = u8> {
    fn extend_from_slice(&mut self, data: &[u8]);
    fn start_offset(&self) -> usize;
    fn end_offset(&self) -> usize;
    fn slice_from(&self, start_off: usize) -> ConcatSlice;
}

/// A byte buffer of fixed length which is typically used to store the last N bytes of some larger
/// stream of bytes.
pub struct CyclicBuffer {
    data: Vec<u8>,

    /// Absolute offset from before the first byte was ever inserted.
    /// This is essentially equivalent to the total number of bytes ever
    /// inserted during the lifetime of this buffer
    end_offset: usize,
}

impl CyclicBuffer {
    pub fn new(size: usize) -> Self {
        assert!(size > 0);
        let mut data = vec![];
        data.resize(size, 0);
        CyclicBuffer {
            data,
            end_offset: 0,
        }
    }
}

impl WindowBuffer for CyclicBuffer {
    fn extend_from_slice(&mut self, mut data: &[u8]) {
        // Skip complete cycles of the buffer if the data is longer than the buffer.
        if data.len() >= self.data.len() {
            let nskip = data.len() - self.data.len();
            data = &data[nskip..];
            self.end_offset += nskip;
        }

        // NOTE: This will only ever have up to two iterations.
        while data.len() > 0 {
            let off = self.end_offset % self.data.len();
            let n = std::cmp::min(self.data.len() - off, data.len());
            (&mut self.data[off..(off + n)]).copy_from_slice(&data[0..n]);

            data = &data[n..];
            self.end_offset += n;
        }
    }

    /// The lowest absolute offset available in this buffer.
    fn start_offset(&self) -> usize {
        if self.end_offset > self.data.len() {
            self.end_offset - self.data.len()
        } else {
            0
        }
    }

    fn end_offset(&self) -> usize {
        self.end_offset
    }

    fn slice_from(&self, start_off: usize) -> ConcatSlice {
        assert!(start_off >= self.start_offset() && start_off <= self.end_offset);

        let off = start_off % self.data.len();
        let mut n = self.end_offset - start_off;

        let rem = std::cmp::min(n, self.data.len() - off);
        let mut s = ConcatSlice::with(&self.data[off..(off + rem)]);
        n -= rem;

        if n > 0 {
            s = s.append(&self.data[0..n]);
        }

        s
    }
}

impl std::ops::Index<usize> for CyclicBuffer {
    type Output = u8;
    fn index(&self, idx: usize) -> &Self::Output {
        assert!(idx >= self.start_offset() && idx < self.end_offset());

        let off = idx % self.data.len();
        &self.data[off]
    }
}

pub struct SliceBuffer<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> SliceBuffer<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
}

impl<'a> WindowBuffer for SliceBuffer<'a> {
    fn extend_from_slice(&mut self, mut data: &[u8]) {
        unsafe {
            assert_eq!(self.data.as_ptr().add(self.pos), data.as_ptr());
        };
        self.pos += data.len();
    }

    fn start_offset(&self) -> usize {
        0
    }
    fn end_offset(&self) -> usize {
        self.pos
    }
    fn slice_from(&self, start_off: usize) -> ConcatSlice {
        ConcatSlice::with(&self.data[start_off..])
    }
}

impl<'a> std::ops::Index<usize> for SliceBuffer<'a> {
    type Output = u8;
    fn index(&self, idx: usize) -> &Self::Output {
        &self.data[idx]
    }
}

/// A slice like object consisting of multiple slices concatenated sequentially.
///
/// TODO: Optimize this for the case of having up to 3-4 concatenated slices
pub struct ConcatSlice<'a> {
    inner: Vec<&'a [u8]>,
}

impl<'a> ConcatSlice<'a> {
    pub fn with(s: &'a [u8]) -> Self {
        ConcatSlice { inner: vec![s] }
    }

    pub fn append(mut self, s: &'a [u8]) -> Self {
        self.inner.push(s);
        self
    }

    pub fn len(&self) -> usize {
        self.inner.iter().map(|s| s.len()).sum()
    }

    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = vec![];
        for piece in &self.inner {
            out.extend_from_slice(piece);
        }

        out
    }
}

impl<'a> std::ops::Index<usize> for ConcatSlice<'a> {
    type Output = u8;
    fn index(&self, idx: usize) -> &Self::Output {
        let mut pos = 0;
        for s in self.inner.iter() {
            if idx - pos < s.len() {
                return &s[idx - pos];
            }

            pos += s.len();
        }

        panic!("Index out of range");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cyclic_buffer_test() {
        let mut b = CyclicBuffer::new(8);
        assert_eq!(b.start_offset(), 0);
        assert_eq!(b.end_offset(), 0);
        
        b.extend_from_slice(&[1,2,3,4]);
        assert_eq!(b.start_offset(), 0);
        assert_eq!(b.end_offset(), 4);
        assert_eq!(b[0], 1);
        assert_eq!(b[2], 3);

        b.extend_from_slice(&[15,16,17,18,19]);

        assert_eq!(b.start_offset(), 1);
        assert_eq!(b.end_offset(), 9);
        assert_eq!(b[1], 2);
        assert_eq!(b[5], 16);
        assert_eq!(&b.slice_from(1).to_vec(), &[2,3,4,15,16,17,18,19]);

        b.extend_from_slice(&[0,0,0,0,0,0,0,0, 0,0,0,0,0,0,0,0, 4, 3, 2, 1]);
        assert_eq!(b.start_offset(), 21);
        assert_eq!(b.end_offset(), 29);
        assert_eq!(&b.slice_from(21).to_vec(), &[0,0,0,0, 4, 3, 2, 1]);
        assert_eq!(&b.slice_from(23).to_vec(), &[0,0, 4, 3, 2, 1]);
        assert_eq!(&b.slice_from(28).to_vec(), &[1]);
        assert_eq!(&b.slice_from(29).to_vec(), &[]);

        // TODO: Also test extend_from_slice() with a zero length slice or a slice that is the exact length of the buffer
    }

}

// TODO: Add lot's of tests to this.
