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

/// Returns the index of the first element with value >= the given target.
/// NOTE: This assumes that 'values' is sorted.
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
