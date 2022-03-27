use core::cmp::PartialOrd;
use core::iter::Iterator;

/// Returns the index of the largest element in a sequence.
pub fn argmax<T: PartialOrd, I: Iterator<Item = usize>, F: Fn(usize) -> T>(
    arg: I,
    func: F,
) -> Option<usize> {
    let mut max = None;
    for i in arg {
        if max.is_none() || func(i) > func(max.unwrap()) {
            max = Some(i)
        }
    }

    max
}
