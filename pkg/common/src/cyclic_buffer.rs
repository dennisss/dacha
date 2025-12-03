// TODO: Dedup with the deflate one.


use core::convert::Infallible;
use core::convert::{AsRef, AsMut};
use core::ops::{Index, IndexMut};

use crate::list::Appendable;

/// FIFO queue for storing up to a fixed number of bytes.
///
/// Note that this data structure will never overwrite old elements
/// until they are removed (instead it will return an error on insertions
/// until there is space available).
pub struct CyclicBuffer<Array> {
    buf: Array,
    start: usize,
    length: usize
}

impl<Array: AsRef<[u8]> + AsMut<[u8]>> CyclicBuffer<Array> {

    pub fn new(buf: Array) -> Self {
        Self {
            buf,
            start: 0,
            length: 0,
        }
    }

    pub fn clear(&mut self) {
        self.start = 0;
        self.length = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn capacity(&self) -> usize {
        self.buf.as_ref().len()
    }

    fn fast_mod(mut v: usize, m: usize) -> usize {
        if v >= m {
            v -= m;
        }

        v
    }

    /// Returns 'true' if the data overflowed the buffer (and wasn't inserted).
    pub fn extend_from_slice(&mut self, mut data: &[u8]) -> bool {
        let buf = self.buf.as_mut();
        let capacity = buf.len();

        let next_length = self.length + data.len();
        if next_length > capacity {
            return true;
        }

        let mut offset = Self::fast_mod(self.start + self.length, capacity);
        self.length = next_length;

        // NOTE: This will only ever loop up to twice.
        while !data.is_empty() {
            let n = data.len().min(capacity - offset);
            buf[offset..(offset + n)].copy_from_slice(&data[0..n]);
            data = &data[n..];
            offset = 0;
        }

        false
    }

    pub fn push(&mut self, v: u8) -> bool {
        // This function is redundant with but a slightly more optimized version of:
        // self.extend_from_slice(core::slice::from_ref(&v));

        let buf = self.buf.as_mut();
        let capacity = buf.len();
        if self.length == capacity {
            return true;
        }

        let offset = Self::fast_mod(self.start + self.length, capacity);
        buf[offset] = v;

        self.length += 1;

        false
    }

    pub fn read(&mut self, mut out: &mut [u8]) -> usize {
        let buf = self.buf.as_ref();
        let capacity = buf.len();

        let mut total_read = 0;

        while self.length > 0 && out.len() > 0 {
            let n = out.len()
                .min(self.length)
                .min(capacity - self.start);
            out[0..n].copy_from_slice(&buf[self.start..(self.start + n)]);

            out = &mut out[n..];
            self.length -= n;
            self.start = Self::fast_mod(self.start + n, capacity);
            total_read += n;
        }

        total_read
    }

    pub fn checkpoint(&mut self) -> CyclicBufferCheckpoint<Array> {
        let initial_length = self.length;

        CyclicBufferCheckpoint {
            buf: self,
            initial_length,
            overflowed: false
        }
    }

}

pub struct CyclicBufferCheckpoint<'a, Array> {
    buf: &'a mut CyclicBuffer<Array>,
    initial_length: usize,
    overflowed: bool,
}

impl<'a, Array: AsRef<[u8]> + AsMut<[u8]>> CyclicBufferCheckpoint<'a, Array> {
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Gets the number of elements that were appended in this checkpoint.
    pub fn len(&self) -> usize {
        self.buf.length - self.initial_length
    }

    fn get_idx(&self, index: usize) -> usize {
        CyclicBuffer::<Array>::fast_mod(self.buf.start + self.initial_length + index, self.buf.capacity())
    }
}

impl<'a, Array> Drop for CyclicBufferCheckpoint<'a, Array> {
    fn drop(&mut self) {
        if self.overflowed {
            self.buf.length = self.initial_length;
        }
    }
}

impl<'a, Array: AsRef<[u8]> + AsMut<[u8]>> Index<usize> for CyclicBufferCheckpoint<'a, Array> {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        let i = self.get_idx(index);
        &self.buf.buf.as_ref()[i]
    }
}

impl<'a, Array: AsRef<[u8]> + AsMut<[u8]>> IndexMut<usize> for CyclicBufferCheckpoint<'a, Array> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        let i = self.get_idx(index);
        &mut self.buf.buf.as_mut()[i]
    }
}

impl<'a, Array: AsRef<[u8]> + AsMut<[u8]>> Appendable for CyclicBufferCheckpoint<'a, Array> {
    type Item = u8;
    type Error = Infallible;

    fn push(&mut self, value: Self::Item) -> Result<(), Self::Error> {
        self.overflowed |= self.buf.push(value);
        Ok(())
    }

    fn extend_from_slice(&mut self, other: &[Self::Item]) -> Result<(), Self::Error> {
        self.overflowed |= self.buf.extend_from_slice(other);
        Ok(())
    }
}



#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn works() {
        let mut buf = CyclicBuffer::new([0u8; 8]);

        buf.push(12);
        assert_eq!(&buf.buf[..], &[12, 0, 0, 0, 0, 0, 0, 0]);

        buf.extend_from_slice(&[1, 2, 3, 4]);
        assert_eq!(&buf.buf[..], &[12, 1, 2, 3, 4, 0, 0, 0]);

        let mut tmp = [0u8; 4];
        assert_eq!(buf.read(&mut tmp), 4);
        assert_eq!(&tmp, &[12, 1, 2, 3]);

        buf.extend_from_slice(&[5, 6, 7, 8]);
        assert_eq!(buf.read(&mut tmp), 4);
        assert_eq!(&tmp, &[4, 5, 6, 7]);

        assert_eq!(buf.read(&mut tmp), 1);
        assert_eq!(&tmp[0..1], &[8]);

        println!("===");
    }

    #[test]
    fn checkpoint() {
        let mut buf = CyclicBuffer::new([0u8; 8]);
        buf.push(5);

        let mut c = buf.checkpoint();
        c.push(10);
        assert!(!c.overflowed());
        assert_eq!(c[0], 10);
        drop(c);

        assert_eq!(&buf.buf[..], &[5, 10, 0, 0, 0, 0, 0, 0]);
        assert_eq!(buf.start, 0);
        assert_eq!(buf.length, 2);

        let mut c = buf.checkpoint();
        for i in 0..20 {
            c.push(i);
        }
        assert!(c.overflowed());
        drop(c);

        assert_eq!(&buf.buf[..], &[5, 10, 0, 1, 2, 3, 4, 5]);
        assert_eq!(buf.start, 0);
        assert_eq!(buf.length, 2);

        let mut tmp = [0u8; 1];
        assert_eq!(buf.read(&mut tmp), 1);
        assert_eq!(tmp[0], 5);

        let mut c = buf.checkpoint();
        c.push(40);
        c.push(41);
        assert!(!c.overflowed());
        assert_eq!(c[0], 40);
        assert_eq!(c[1], 41);
        drop(c);

        assert_eq!(&buf.buf[..], &[5, 10, 40, 41, 2, 3, 4, 5]);
        assert_eq!(buf.start, 1);
        assert_eq!(buf.length, 3);

    }

}
