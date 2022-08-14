/// A cyclic buffer for storing byte data which is canonically read/written in
/// dynamic length segments/packets.
///
/// Internally the implementation is similar to a cyclic byte buffer except that
/// each segment is prefixed by an encoded length integer defining how long the
/// following segment is.
///
/// Currently an 8-bit length prefix is used so all data packets must be <= 255
/// bytes in length.
pub struct SegmentedBuffer<Array> {
    start: usize,
    length: usize,
    buf: Array,
}

impl<Array: AsRef<[u8]> + AsMut<[u8]>> SegmentedBuffer<Array> {
    pub const fn new(buffer: Array) -> Self {
        Self {
            start: 0,
            length: 0,
            buf: buffer,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn clear(&mut self) {
        self.start = 0;
        self.length = 0;
    }

    pub fn write(&mut self, mut segment: &[u8]) {
        assert!(segment.len() + 1 <= self.buf.as_ref().len());

        let len = segment.len() as u8;
        assert_eq!(self.append_partial(core::slice::from_ref(&len)), 1);

        while segment.len() > 0 {
            let n = self.append_partial(segment);
            segment = &segment[n..];
        }
    }

    fn append_partial(&mut self, data: &[u8]) -> usize {
        let i = (self.start + self.length) % self.buf.as_ref().len();
        let j = core::cmp::min(i + data.len(), self.buf.as_ref().len());
        let n = j - i;

        // Check if we are about to overrun the start of the oldest segment. If so, we
        // need to advance it forward.
        while self.length > 0 && self.start >= i && self.start < j {
            let first_segment_len = self.buf.as_ref()[self.start] as usize;
            self.start += 1;
            self.length -= 1;

            self.start += first_segment_len;
            self.length -= first_segment_len;

            self.start = self.start % self.buf.as_ref().len();
        }

        self.buf.as_mut()[i..j].copy_from_slice(&data[0..n]);
        self.length += n;

        n
    }

    /// Checks what the size of the
    pub fn peek(&self) -> Option<usize> {
        if self.length == 0 {
            return None;
        }

        let len = self.buf.as_ref()[self.start] as usize;
        Some(len)
    }

    /// Removes the first segment from the buffer.
    pub fn read(&mut self, mut out: &mut [u8]) -> Option<usize> {
        if self.length == 0 {
            return None;
        }

        let len = self.buf.as_ref()[self.start] as usize;
        self.start = (self.start + 1) % self.buf.as_ref().len();
        self.length -= 1;

        assert!(out.len() >= len);

        let mut remaining = len;
        while remaining > 0 {
            let i = self.start;
            let j = core::cmp::min(i + remaining, self.buf.as_ref().len());
            let n = j - i;

            out[0..n].copy_from_slice(&self.buf.as_ref()[i..j]);
            self.start = j % self.buf.as_ref().len();
            self.length -= n;

            remaining -= n;
            out = &mut out[n..];
        }

        Some(len)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn insert_and_delete_from_empty_buffer() {
        let mut buf = SegmentedBuffer::new([0u8; 256]);
        assert!(buf.is_empty());

        buf.write(&[1, 2, 3]);
        assert!(!buf.is_empty());

        let mut out = [0u8; 20];
        assert_eq!(buf.read(&mut out), Some(3));
        assert_eq!(&out[0..3], &[1, 2, 3]);
        assert!(buf.is_empty());

        buf.write(&[4, 5]);
        assert!(!buf.is_empty());

        assert_eq!(buf.read(&mut out), Some(2));
        assert_eq!(&out[0..2], &[4, 5]);
    }

    #[test]
    fn insert_multiple() {
        let mut buf = SegmentedBuffer::new([0u8; 20]);
        buf.write(&[5, 4, 3, 2]);
        buf.write(&[]);
        buf.write(&[22, 21]);
        buf.write(&[1]);

        let mut out = [0u8; 8];
        assert_eq!(buf.read(&mut out), Some(4));
        assert_eq!(&out[0..4], &[5, 4, 3, 2]);

        assert_eq!(buf.read(&mut out), Some(0));

        assert_eq!(buf.read(&mut out), Some(2));
        assert_eq!(&out[0..2], &[22, 21]);

        assert_eq!(buf.read(&mut out), Some(1));
        assert_eq!(&out[0..1], &[1]);

        assert_eq!(buf.read(&mut out), None);
    }

    #[test]
    fn overrun_old_data() {
        let mut buf = SegmentedBuffer::new([0u8; 8]);

        // These 3 writes consume 6 out of 8 bytes.
        buf.write(&[1]);
        buf.write(&[2]);
        buf.write(&[3]);

        buf.write(&[4, 5, 6, 7]);

        let mut out = [0u8; 8];
        assert_eq!(buf.read(&mut out), Some(1));
        assert_eq!(&out[0..1], &[3]);

        assert_eq!(buf.read(&mut out), Some(4));
        assert_eq!(&out[0..4], &[4, 5, 6, 7]);

        assert_eq!(buf.read(&mut out), None);
    }
}
