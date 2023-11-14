use std::cmp::Ordering;
use std::sync::Arc;

use common::errors::*;
use common::tree::binary_heap::BinaryHeap;
use common::tree::comparator::Comparator;

use crate::iterable::{Iterable, KeyValueEntry};
use crate::table::comparator::KeyComparator;

/// Iterable wrapper around multiple Iterable objects created by iterating
/// through them in sorted order.
pub struct MergeIterator {
    pending_iters: Vec<Box<dyn Iterable<KeyValueEntry>>>,
    next_queue: BinaryHeap<MergeIteratorEntry, MergeKeysComparator>,
    exhausted_iters: Vec<Box<dyn Iterable<KeyValueEntry>>>,
}

struct MergeIteratorEntry {
    inner: Box<dyn Iterable<KeyValueEntry>>,
    next_entry: KeyValueEntry,
}

struct MergeKeysComparator {
    key_comparator: Arc<dyn KeyComparator>,
}

#[async_trait]
impl Comparator<MergeIteratorEntry> for MergeKeysComparator {
    fn compare(&self, a: &MergeIteratorEntry, b: &MergeIteratorEntry) -> Ordering {
        self.key_comparator
            .compare(&a.next_entry.key, &b.next_entry.key)
    }
}

impl MergeIterator {
    pub fn new(
        key_comparator: Arc<dyn KeyComparator>,
        iterators: Vec<Box<dyn Iterable<KeyValueEntry>>>,
    ) -> Self {
        let next_queue = BinaryHeap::new(MergeKeysComparator { key_comparator }, ());
        let exhausted_iters = vec![];

        // NOTE: We don't add the iterators to the queue until the user requests
        // a key to ensure that the user can seek first.
        Self {
            pending_iters: iterators,
            next_queue,
            exhausted_iters,
        }
    }
}

#[async_trait]
impl Iterable<KeyValueEntry> for MergeIterator {
    // TODO: When calling .next() on an inner iterator, pass it a min key hint such
    // that it stops early if we know that it doesn't have a smaller key than what
    // is in our priority queue.

    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        // Add all pending_iters to the queue.
        while let Some(mut iter) = self.pending_iters.pop() {
            let next_entry = iter.next().await?;
            if let Some(next_entry) = next_entry {
                self.next_queue.insert(MergeIteratorEntry {
                    next_entry,
                    inner: iter,
                });
            } else {
                self.exhausted_iters.push(iter);
            }
        }

        if let Some(mut entry) = self.next_queue.extract_min() {
            if let Some(next_entry) = entry.inner.next().await? {
                // TODO: Optimize this by performing a decrease-key instead.
                self.next_queue.insert(MergeIteratorEntry {
                    next_entry,
                    inner: entry.inner,
                });
            } else {
                self.exhausted_iters.push(entry.inner);
            }

            return Ok(Some(entry.next_entry));
        }

        Ok(None)
    }

    async fn seek(&mut self, key: &[u8]) -> Result<()> {
        // Move all iterators to the pending_iters list.
        while let Some(iter) = self.next_queue.pop_any() {
            self.pending_iters.push(iter.inner);
        }
        while let Some(iter) = self.exhausted_iters.pop() {
            self.pending_iters.push(iter);
        }

        for iter in &mut self.pending_iters {
            iter.seek(key).await?;
        }

        Ok(())
    }
}
