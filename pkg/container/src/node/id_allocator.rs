use std::collections::HashSet;

use common::hash::FastHasherBuilder;

use super::shadow::IdRange;

/// For allocating unique Linux uid/gid.
pub struct IdAllocator {
    next_id: u32,
    range: IdRange,
    allocated: HashSet<u32, FastHasherBuilder>,
}

impl IdAllocator {
    pub fn new(range: IdRange) -> Self {
        Self {
            next_id: range.start_id,
            range,
            allocated: HashSet::default(),
        }
    }

    pub fn reserve(&mut self, id: u32) {
        self.allocated.insert(id);
    }

    pub fn allocate(&mut self) -> Option<u32> {
        let first_id = self.next_id;

        loop {
            if self.allocated.insert(self.next_id) {
                return Some(self.next_id);
            }

            self.next_id += 1;

            if self.next_id >= self.range.start_id + self.range.count {
                self.next_id = self.range.start_id;
            }

            if self.next_id == first_id {
                return None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn works() {
        let mut a = IdAllocator::new(IdRange {
            start_id: 200,
            count: 4,
        });
        assert_eq!(a.allocate(), Some(200));
        assert_eq!(a.allocate(), Some(201));
        assert_eq!(a.allocate(), Some(202));
        assert_eq!(a.allocate(), Some(203));
        assert_eq!(a.allocate(), None);
    }

    #[test]
    fn reserve_port() {
        let mut a = IdAllocator::new(IdRange {
            start_id: 200,
            count: 4,
        });
        a.reserve(200);
        a.reserve(202);
        assert_eq!(a.allocate(), Some(201));
        assert_eq!(a.allocate(), Some(203));
        assert_eq!(a.allocate(), None);
    }
}
