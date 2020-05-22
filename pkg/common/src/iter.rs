use std::convert::AsRef;
use std::iter::Iterator;

pub trait PairIter<T> {
    /// Creates an iterator which iterates over pairs of consecutive elements.
    /// e.g. &[1,2,3].pair_iter() will produce the following elements:
    /// => (&1, &2), (&2, &3)
    fn pair_iter(&self) -> PairIterator<T>;
}

impl<T, Y: AsRef<[T]>> PairIter<T> for Y {
    fn pair_iter(&self) -> PairIterator<T> {
        PairIterator {
            slice: self.as_ref(),
            i: 0,
            reverse: false,
        }
    }
}

pub struct PairIterator<'a, T> {
    slice: &'a [T],
    i: usize,
    reverse: bool,
}

impl<'a, T> PairIterator<'a, T> {
    pub fn rev(mut self) -> Self {
        self.reverse = !self.reverse;
        self.i = (self.slice.len() - 1) - self.i;
        self
    }
}

impl<'a, T> Iterator for PairIterator<'a, T> {
    type Item = (&'a T, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        if self.reverse {
            if self.i >= 1 {
                let pair = (&self.slice[self.i], &self.slice[self.i - 1]);
                self.i -= 1;
                Some(pair)
            } else {
                None
            }
        } else {
            if self.i + 1 < self.slice.len() {
                let pair = (&self.slice[self.i], &self.slice[self.i + 1]);
                self.i += 1;
                Some(pair)
            } else {
                None
            }
        }
    }
}
