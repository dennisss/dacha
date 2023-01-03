pub use self::disjoint_sets::DisjointSets;
pub use self::merge::*;
use std::ops::Deref;
use std::ops::{Index, Range, RangeFrom};
use std::slice::SliceIndex;

mod disjoint_sets;
mod merge;

pub trait SliceLike {
    type Item;

    fn len(&self) -> usize;

    fn index(&self, idx: usize) -> Self::Item;

    fn slice(&self, start: usize, end: usize) -> Self;

    fn slice_from(&self, start: usize) -> Self;
}

impl<'a, T> SliceLike for &'a [T] {
    type Item = &'a T;

    fn len(&self) -> usize {
        (*self).len()
    }

    fn index(&self, idx: usize) -> Self::Item {
        Index::index(*self, idx)
    }

    /// self[start..end]
    fn slice(&self, start: usize, end: usize) -> Self {
        &self[start..end]
    }

    /// self[start..]
    fn slice_from(&self, start: usize) -> Self {
        &self[start..]
    }
}

// Implementation for a pair of slices which should be treated as though they
// were concatenated. This enables casting a VecDeque as a SliceLike.
impl<'a, T> SliceLike for (&'a [T], &'a [T]) {
    type Item = &'a T;

    fn len(&self) -> usize {
        self.0.len() + self.1.len()
    }

    fn index(&self, idx: usize) -> Self::Item {
        if idx < self.0.len() {
            &self.0[idx]
        } else {
            &self.1[idx - self.0.len()]
        }
    }

    fn slice(&self, start: usize, end: usize) -> Self {
        let mid = self.0.len();
        let first = {
            let i = std::cmp::min(start, mid);
            let j = std::cmp::min(end, mid);

            &self.0[i..j]
        };
        let second = {
            let i = std::cmp::max(start, mid) - mid;
            let j = std::cmp::max(end, mid) - mid;

            &self.1[i..j]
        };

        (first, second)
    }

    fn slice_from(&self, start: usize) -> Self {
        self.slice(start, self.len())
    }
}

/// Returns the index of the first element with value >= the given target.
/// NOTE: This assumes that 'values' is sorted.
///
/// If None is returned, then all values are < the target.
pub fn lower_bound<T: PartialOrd>(values: &[T], target: &T) -> Option<usize> {
    lower_bound_by(values, target, |a, b| *a >= *b)
}

pub fn lower_bound_by<T: Copy, S: SliceLike, F: Fn(<S as SliceLike>::Item, T) -> bool>(
    mut values: S,
    target: T,
    greater_eq: F,
) -> Option<usize> {
    let mut best = None;
    let mut offset = 0;
    loop {
        if values.len() == 0 {
            return best;
        }

        let mid_idx = values.len() / 2;
        if greater_eq(values.index(mid_idx), target) {
            best = Some(mid_idx + offset);
            values = values.slice(0, mid_idx);
        } else {
            // NOTE: We can skip mid_idx because it is known to be < the target.
            values = values.slice_from(mid_idx + 1);
            offset += mid_idx + 1;
        }
    }
}

/// Returns the index of the last element with value <= the given target.
/// NOTE: This assumes that 'values' is sorted in ascending order.
///
/// If None is returned, then all the elements are > the target.
pub fn upper_bound<T: PartialOrd>(values: &[T], target: &T) -> Option<usize> {
    upper_bound_by(values, target, |a, b| *a <= *b)
}

pub fn upper_bound_by<T: Copy, S: SliceLike, F: Fn(<S as SliceLike>::Item, T) -> bool>(
    mut values: S,
    target: T,
    less_eq: F,
) -> Option<usize> {
    let mut best = None;
    let mut offset = 0;
    loop {
        if values.len() == 0 {
            return best;
        }

        let mid_idx = values.len() / 2;
        if less_eq(values.index(mid_idx), target) {
            best = Some(mid_idx + offset);
            // NOTE: We are skipping mid_idx as we already know it is <= the target.
            values = values.slice_from(mid_idx + 1);
            offset += mid_idx + 1;
        } else {
            values = values.slice(0, mid_idx);
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn lower_bound_test() {
        let values = &[10, 20, 30, 40, 50, 60];

        assert_eq!(lower_bound(values, &5), Some(0));
        assert_eq!(lower_bound(values, &10), Some(0));
        assert_eq!(lower_bound(values, &12), Some(1));
        assert_eq!(lower_bound(values, &40), Some(3));
        assert_eq!(lower_bound(values, &49), Some(4));
        assert_eq!(lower_bound(values, &70), None);
    }

    #[test]
    fn upper_bound_test() {
        let values = &[10, 20, 30, 40, 50, 60];

        assert_eq!(upper_bound(values, &5), None);
        assert_eq!(upper_bound(values, &10), Some(0));
        assert_eq!(upper_bound(values, &12), Some(0));
        assert_eq!(upper_bound(values, &40), Some(3));
        assert_eq!(upper_bound(values, &49), Some(3));
        assert_eq!(upper_bound(values, &70), Some(5));
    }
}
