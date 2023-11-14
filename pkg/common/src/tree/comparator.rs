use core::cmp::Ordering;

pub trait Comparator<A, B = A> {
    fn compare(&self, a: &A, b: &B) -> Ordering;
}

#[derive(Default)]
pub struct OrdComparator {
    _hidden: (),
}

impl<T: Ord> Comparator<T> for OrdComparator {
    fn compare(&self, a: &T, b: &T) -> Ordering {
        a.cmp(b)
    }
}
