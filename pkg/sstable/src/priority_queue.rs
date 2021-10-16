use std::cmp::Ordering;

pub trait Comparator<T>: Send + 'static {
    fn compare(&self, a: &T, b: &T) -> Ordering;
}

/// TODO: Switch to using a Fibonacci heap.
pub struct PriorityQueue<T> {
    items: Vec<T>,
    comparator: Box<dyn Comparator<T>>,
}

impl<T: 'static> PriorityQueue<T> {
    // TODO: Implement O(n) creation of a new heap.
    pub fn new(comparator: Box<dyn Comparator<T>>) -> Self {
        Self {
            items: vec![],
            comparator,
        }
    }

    fn compare(&self, a: &T, b: &T) -> Ordering {
        self.comparator.compare(a, b)
    }

    pub fn insert(&mut self, value: T) {
        self.items.push(value);

        let mut i = self.items.len() - 1;
        while i > 0 {
            let parent_i = ((i + 1) / 2) - 1;
            if self.compare(&self.items[i], &self.items[parent_i]).is_ge() {
                break;
            }

            self.items.swap(i, parent_i);
            i = parent_i;
        }
    }

    pub fn extract_min(&mut self) -> Option<T> {
        if self.items.len() <= 1 {
            return self.items.pop();
        }

        let v = self.items.swap_remove(0);

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
                self.items.swap(i, min_i);
                i = min_i;
            } else {
                break;
            }
        }

        Some(v)
    }

    /// Removes any arbitrary entry from the queue.
    pub fn pop_any(&mut self) -> Option<T> {
        self.items.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UsizeComparator {}

    impl Comparator<usize> for UsizeComparator {
        fn compare(&self, a: &usize, b: &usize) -> Ordering {
            a.cmp(b)
        }
    }

    #[test]
    fn priority_queue_test() {
        let mut queue = PriorityQueue::new(Box::new(UsizeComparator {}));

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
        let mut queue = PriorityQueue::new(Box::new(UsizeComparator {}));

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