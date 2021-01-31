/// In this module, we define the ArenaStack data type.
///
/// This is a dynamically sized stack where all elements are stored in a fixed
/// size arena that may be shared by many other lists. Internally this is
/// implemented as a doubly linked list. Each element of the list is referenced
/// by an index into the arena. Each element of the arena should only belong to
/// up to one list,
use core::marker::PhantomData;

pub type ArenaIndex = u8;

pub trait Arena<T> {
    fn get(&self, index: ArenaIndex) -> T;
    fn set(&self, index: ArenaIndex, value: T);

    fn update<F: FnOnce(&mut T)>(&self, index: ArenaIndex, f: F) {
        let mut value = self.get(index);
        f(&mut value);
        self.set(index, value);
    }
}

// TODO: Remove Clone/Copy.
#[derive(Clone, Copy, Debug)]
#[repr(packed)]
pub struct ArenaStackItem<T> {
    /// Index of the previous entry in this linked list.
    /// If equal to the index of the current item, then there is no previous
    /// entry.
    prev: ArenaIndex,

    /// Index of the next entry in this linked list.
    /// If equal to the index of the current item, then there is no next entry.
    next: ArenaIndex,

    /// Some value associated with this item.
    value: T,
}

impl<T> ArenaStackItem<T> {
    pub const fn empty(value: T) -> Self {
        Self {
            prev: 0,
            next: 0,
            value,
        }
    }
}

pub struct ArenaStack<T, A: Arena<ArenaStackItem<T>>> {
    arena: A,

    /// Index of the first/last item in this list (if any).
    head: Option<ArenaIndex>,

    marker: PhantomData<T>,
}

impl<T, A: Arena<ArenaStackItem<T>>> ArenaStack<T, A> {
    pub const fn new(arena: A) -> Self {
        // TODO: Validate that the ArenaIndex is large enough to address all arena
        // items.

        Self {
            arena,
            head: None,
            marker: PhantomData,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    pub fn push(&mut self, index: ArenaIndex, value: T) {
        let old_head_index = {
            if let Some(head_index) = self.head.take() {
                self.arena
                    .update(head_index, |old_head| old_head.next = index);
                head_index
            } else {
                // Meaning that there was no head.
                index
            }
        };

        self.head = Some(index);

        self.arena.set(
            index,
            ArenaStackItem {
                prev: old_head_index,
                next: index,
                value,
            },
        );
    }

    /// NOTE: We do not validate the 'index' is actually owned by this list.
    /// Returns the previous item before the removed one.
    pub fn remove(&mut self, index: ArenaIndex) -> Option<(T, ArenaIndex)> {
        let mut next_idx = self.arena.get(index).next;
        let prev_idx = self.arena.get(index).prev;

        // This means that we are removing the head
        if next_idx == index {
            assert!(Some(index) == self.head);

            self.head = if prev_idx == index {
                None
            } else {
                // This will be used later in this function to mark that there is no item after
                // the previous item (if there is a previous item).
                next_idx = prev_idx;
                Some(prev_idx)
            };
        } else {
            self.arena.update(next_idx, |next_item| {
                next_item.prev = if prev_idx == index {
                    next_idx
                } else {
                    prev_idx
                };
            });
        }

        if prev_idx != index {
            self.arena
                .update(prev_idx, |prev_item| prev_item.next = next_idx);

            return Some((self.arena.get(prev_idx).value, prev_idx));
        } else {
            return None;
        }
    }

    /// Reads the value and index of the item at the head of the stack
    /// Doesn't mutate the stack in any way.
    pub fn peek(&self) -> Option<(T, ArenaIndex)> {
        if let Some(head_index) = self.head {
            let head = self.arena.get(head_index);
            return Some((head.value, head_index));
        }

        None
    }

    pub fn pop(&mut self) -> Option<(T, ArenaIndex)> {
        self.peek().map(|(value, index)| {
            self.remove(index);
            (value, index)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::vec::Vec;

    struct IntArena {
        values: Vec<Cell<ArenaStackItem<u32>>>,
    }

    impl IntArena {
        fn new() -> Self {
            let mut values = vec![];
            for i in 0..8 {
                values.push(Cell::new(ArenaStackItem::empty(0)));
            }

            Self { values }
        }
    }

    impl Arena<ArenaStackItem<u32>> for &IntArena {
        fn get(&self, index: ArenaIndex) -> ArenaStackItem<u32> {
            self.values[index as usize].get()
        }
        fn set(&self, index: ArenaIndex, value: ArenaStackItem<u32>) {
            self.values[index as usize].set(value);
        }
    }

    #[test]
    fn push_and_pop() {
        let arena = IntArena::new();
        let mut list = ArenaStack::new(&arena);

        assert_eq!(list.peek(), None);
        assert_eq!(list.pop(), None);

        list.push(0, 10);
        list.push(1, 11);
        list.push(2, 12);

        assert_eq!(list.peek(), Some((12, 2)));
        assert_eq!(list.peek(), Some((12, 2)));
        assert_eq!(list.pop(), Some((12, 2)));

        assert_eq!(list.peek(), Some((11, 1)));
        assert_eq!(list.pop(), Some((11, 1)));

        assert_eq!(list.pop(), Some((10, 0)));
        assert_eq!(list.peek(), None);
        assert_eq!(list.pop(), None);
    }

    fn remove_middle() {
        let arena = IntArena::new();
        let mut list = ArenaStack::new(&arena);
        list.push(0, 10);
        list.push(1, 11);
        list.push(2, 12);

        assert_eq!(list.remove(1), Some((10, 0)));

        assert_eq!(list.pop(), Some((12, 2)));
        assert_eq!(list.pop(), Some((10, 0)));
        assert_eq!(list.pop(), None);
    }

    fn remove_start() {
        let arena = IntArena::new();
        let mut list = ArenaStack::new(&arena);
        list.push(0, 10);
        list.push(1, 11);
        list.push(2, 12);

        assert_eq!(list.remove(0), None);

        assert_eq!(list.pop(), Some((12, 2)));
        assert_eq!(list.pop(), Some((11, 0)));
        assert_eq!(list.pop(), None);
    }

    fn remove_end() {
        let arena = IntArena::new();
        let mut list = ArenaStack::new(&arena);
        list.push(0, 10);
        list.push(1, 11);
        list.push(2, 12);

        assert_eq!(list.remove(2), Some((11, 1)));

        assert_eq!(list.pop(), Some((11, 1)));
        assert_eq!(list.pop(), Some((10, 0)));
        assert_eq!(list.pop(), None);
    }
}
