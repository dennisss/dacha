use alloc::boxed::Box;
use alloc::vec::Vec;
use core::cmp::Ordering;

use crate::tree::comparator::*;

/// Abstract data structure for keeping a mapping from to some offset into
/// another data structure (like a binary heap which naturally does not support
/// random lookup within an external like this).
///
/// (so this operates sort of like an inverted index)
pub trait BinaryHeapIndex<V> {
    type Query;

    fn record_offset(&mut self, value: &V, offset: usize);

    fn lookup_offset(&self, query: &Self::Query) -> Option<usize>;

    fn clear_offset(&mut self, value: &V);
}

impl<V> BinaryHeapIndex<V> for () {
    type Query = ();

    fn record_offset(&mut self, value: &V, offset: usize) {}

    fn lookup_offset(&self, query: &Self::Query) -> Option<usize> {
        None
    }

    fn clear_offset(&mut self, value: &V) {}
}

impl<T: Ord> BinaryHeap<T, OrdComparator, ()> {
    pub fn default() -> Self {
        Self::new(OrdComparator::default(), ())
    }
}

pub struct BinaryHeap<V, C = OrdComparator, Index = ()> {
    items: Vec<V>,
    comparator: C,
    index: Index,
}

impl<V, C: Default, I: Default> Default for BinaryHeap<V, C, I> {
    fn default() -> Self {
        Self {
            items: vec![],
            comparator: C::default(),
            index: I::default(),
        }
    }
}

impl<T, C: Comparator<T>, Index: BinaryHeapIndex<T>> BinaryHeap<T, C, Index> {
    // TODO: Implement O(n) creation of a new heap.
    pub fn new(comparator: C, index: Index) -> Self {
        Self {
            items: vec![],
            comparator,
            index,
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        self.items.reserve(additional)
    }

    pub fn reserve_exact(&mut self, additional: usize) {
        self.items.reserve_exact(additional)
    }

    pub fn index(&self) -> &Index {
        &self.index
    }

    pub fn index_mut(&mut self) -> &mut Index {
        &mut self.index
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn compare(&self, a: &T, b: &T) -> Ordering {
        self.comparator.compare(a, b)
    }

    // BEGIN: Helpers for mutating self.items. All operations that mutate self.items
    // should use these to ensure that indexes are tracked.

    fn swap(&mut self, a: usize, b: usize) {
        self.items.swap(a, b);
        self.index.record_offset(&self.items[a], a);
        self.index.record_offset(&self.items[b], b);
    }

    fn swap_remove(&mut self, index: usize) -> T {
        self.swap(index, self.items.len() - 1);
        self.pop_any().unwrap()
    }

    fn pop(&mut self) -> Option<T> {
        let v = self.items.pop();
        let idx = self.items.len();
        if let Some(value) = &v {
            self.index.clear_offset(value);
        }

        v
    }

    // END
    ///////

    pub fn insert(&mut self, value: T) {
        self.index.record_offset(&value, self.items.len());
        self.items.push(value);

        let mut i = self.items.len() - 1;
        while i > 0 {
            let parent_i = ((i + 1) / 2) - 1;
            if self.compare(&self.items[i], &self.items[parent_i]).is_ge() {
                break;
            }

            self.swap(i, parent_i);
            i = parent_i;
        }
    }

    pub fn peek_min(&self) -> Option<&T> {
        self.items.get(0)
    }

    fn remove_at_index(&mut self, index: usize) -> Option<T> {
        if self.items.len() <= 1 {
            return self.pop();
        }

        let v = self.swap_remove(0);

        let mut i = 0;
        loop {
            let left_child_i = 2 * (i + 1) - 1;
            let right_child_j = left_child_i + 1;

            let mut min_i = i;

            if left_child_i < self.items.len()
                && self
                    .compare(&self.items[left_child_i], &self.items[min_i])
                    .is_lt()
            {
                min_i = left_child_i;
            }

            if right_child_j < self.items.len()
                && self
                    .compare(&self.items[right_child_j], &self.items[min_i])
                    .is_lt()
            {
                min_i = right_child_j;
            }

            if min_i != i {
                self.swap(i, min_i);

                // Swap in keys
                i = min_i;
            } else {
                break;
            }
        }

        Some(v)
    }

    pub fn extract_min(&mut self) -> Option<T> {
        self.remove_at_index(0)
    }

    /// Removes a specific
    pub fn remove(&mut self, query: &Index::Query) -> Option<T> {
        let offset = match self.index.lookup_offset(query) {
            Some(v) => v,
            None => return None,
        };

        self.remove_at_index(offset)
    }

    /// Removes any arbitrary entry from the queue.
    pub fn pop_any(&mut self) -> Option<T> {
        self.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_queue_test() {
        let mut queue = BinaryHeap::default();

        queue.insert(1);
        assert_eq!(queue.extract_min(), Some(1));

        queue.insert(10);
        queue.insert(15);
        queue.insert(12);
        queue.insert(2);
        queue.insert(4);
        queue.insert(13);
        queue.insert(100);

        assert_eq!(queue.extract_min(), Some(2));
        assert_eq!(queue.extract_min(), Some(4));
        assert_eq!(queue.extract_min(), Some(10));
        assert_eq!(queue.extract_min(), Some(12));
        assert_eq!(queue.extract_min(), Some(13));
        assert_eq!(queue.extract_min(), Some(15));
        assert_eq!(queue.extract_min(), Some(100));
    }

    #[test]
    fn priority_queue_reinsert_test() {
        let mut queue = BinaryHeap::default();

        queue.insert(1);
        queue.insert(10);
        queue.insert(5);
        queue.insert(7);
        queue.insert(2);

        assert_eq!(queue.extract_min(), Some(1));

        queue.insert(1);
        assert_eq!(queue.extract_min(), Some(1));

        queue.insert(4);
        assert_eq!(queue.extract_min(), Some(2));
    }
}
