use alloc::vec::Vec;

use crate::number::Cast;

/// Given two shapes which will be used in a coefficient wise operation, obtains
/// the new shape which is the result of that operation.
///
/// The shapes must be 'compatible' meaning that each dimension:
/// - Is equal between 'a' and 'b'
/// - Or one of them is 1.
///
/// If the shapes are not the same length, then the shapes are interprated as
/// left padded.
///
/// Returns None if the shapes are incompatible.
pub fn broadcast_shapes(a: &[usize], b: &[usize]) -> Option<Vec<usize>> {
    let n = core::cmp::max(a.len(), b.len());

    let mut s = vec![];
    s.reserve_exact(n);

    for i in 0..n {
        let a_i = {
            if a.len() + i >= n {
                a[(a.len() + i) - n]
            } else {
                1
            }
        };

        let b_i = {
            if b.len() + i >= n {
                b[(b.len() + i) - n]
            } else {
                1
            }
        };

        let s_i = {
            if a_i == 1 {
                b_i
            } else if b_i == 1 {
                a_i
            } else if a_i == b_i {
                a_i
            } else {
                return None;
            }
        };

        s.push(s_i);
    }

    Some(s)
}

pub fn broadcasted_source_index<I: Copy + Cast<usize>>(
    broadcasted_index: &[I],
    source_shape: &[usize],
) -> Vec<usize> {
    assert!(broadcasted_index.len() >= source_shape.len());

    let mut idx = vec![];
    idx.reserve_exact(source_shape.len());
    for i in 0..source_shape.len() {
        idx.push(if source_shape[i] == 1 {
            0
        } else {
            broadcasted_index[(broadcasted_index.len() - source_shape.len()) + i].cast()
        });
    }

    idx
}
