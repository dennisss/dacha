use core::mem::MaybeUninit;
use core::ops::{Index, IndexMut};

// TODO: Dedup this with the CyclicBuffer code.
pub struct FixedQueue<T, const LEN: usize> {
    pub(super) data: [MaybeUninit<T>; LEN],
    pub(super) offset: usize,
    pub(super) length: usize,
}

impl<T, const LEN: usize> FixedQueue<T, LEN> {
    pub const fn new() -> Self {
        Self {
            data: MaybeUninit::uninit_array(),
            offset: 0,
            length: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.length
    }

    pub fn capacity(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn is_full(&self) -> bool {
        self.length == self.data.len()
    }

    /// NOTE: If the queue is full, this will panic.
    pub fn push_back(&mut self, value: T) {
        assert!(!self.is_full());

        let next_index = (self.offset + self.length) % self.data.as_ref().len();
        self.data[next_index] = MaybeUninit::new(value);
        self.length += 1;
    }

    pub fn pop_front(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let mut v = MaybeUninit::uninit();
        core::mem::swap(&mut v, unsafe { self.data.get_unchecked_mut(self.offset) });
        self.offset = (self.offset + 1) % self.data.len();
        self.length -= 1;
        Some(unsafe { v.assume_init() })
    }

    pub fn clear(&mut self) {
        while let Some(v) = self.pop_front() {
            drop(v);
        }
    }

    pub fn into_iter(self) -> IntoIter<T, LEN> {
        IntoIter { queue: self }
    }

    pub fn first_mut(&mut self) -> Option<&mut T> {
        if self.length == 0 {
            return None;
        }

        let idx = self.offset;

        Some(unsafe { self.data.get_unchecked_mut(idx).assume_init_mut() })
    }
}

impl<T, const LEN: usize> Default for FixedQueue<T, LEN> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const LEN: usize> Drop for FixedQueue<T, LEN> {
    fn drop(&mut self) {
        for i in 0..self.length {
            let idx = (self.offset + i) % self.data.len();
            unsafe { self.data[idx].assume_init_drop() };
        }
    }
}

impl<T, const LEN: usize> Index<usize> for FixedQueue<T, LEN> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        assert!(index < self.length);
        let i = (self.offset + index) % self.data.len();
        unsafe { self.data[i].assume_init_ref() }
    }
}

impl<T, const LEN: usize> IndexMut<usize> for FixedQueue<T, LEN> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        assert!(index < self.length);
        let i = (self.offset + index) % self.data.len();
        unsafe { self.data[i].assume_init_mut() }
    }
}

pub struct IntoIter<T, const LEN: usize> {
    queue: FixedQueue<T, LEN>,
}

impl<T, const LEN: usize> Iterator for IntoIter<T, LEN> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        self.queue.pop_front()
    }
}
