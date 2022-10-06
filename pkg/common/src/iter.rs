use core::convert::AsRef;
use core::iter::Iterator;

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

struct CartesianProductIterator<'a, 'b, A, B> {
    a: &'a [A],
    b: &'b [B],
    next_index: usize,
}

impl<'a, 'b, A, B> Iterator for CartesianProductIterator<'a, 'b, A, B> {
    type Item = (&'a A, &'b B);

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.a.len() * self.b.len() {
            return None;
        }

        let a_i = self.next_index / self.b.len();
        let b_i = self.next_index % self.b.len();
        self.next_index += 1;

        Some((&self.a[a_i], &self.b[b_i]))
    }
}

pub fn cartesian_product<'a, 'b, A, B>(
    a: &'a [A],
    b: &'b [B],
) -> impl Iterator<Item = (&'a A, &'b B)> {
    CartesianProductIterator {
        a,
        b,
        next_index: 0,
    }
}
