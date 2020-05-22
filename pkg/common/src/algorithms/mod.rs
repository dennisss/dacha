pub use self::disjoint_sets::DisjointSets;
pub use self::merge::*;

mod disjoint_sets;
mod merge;

/// Returns the index of the first element with value >= the given target.
/// NOTE: This assumes that 'values' is sorted.
pub fn lower_bound<T: PartialOrd>(mut values: &[T], target: T) -> Option<usize> {
    let mut best = None;
    let mut offset = 0;
    loop {
        if values.len() == 0 {
            return best;
        }

        let mid_idx = values.len() / 2;
        if values[mid_idx] >= target {
            best = Some(mid_idx + offset);
            values = &values[0..mid_idx];
        } else {
            // NOTE: We can skip mid_idx because it is known to be < the target.
            values = &values[(mid_idx + 1)..];
            offset += mid_idx + 1;
        }
    }
}
