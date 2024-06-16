use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use crypto::random::{MersenneTwisterRng, RngExt, SharedRngExt};
use executor::sync::{AsyncMutex, SyncMutex};

use crate::memtable::atomic::AtomicArc;
use crate::{
    iterable::{Iterable, KeyValueEntry},
    table::{comparator::AlwaysGreaterComparator, KeyComparator},
};

const HEIGHT: usize = 8;

/// Memtable implemented as a skip list.
///
/// This is optimized for sequential single threaded inserts with lock free
/// concurrent reads.
pub struct SkipListMemtable {
    comparator: Arc<dyn KeyComparator>,

    /// Node with the smallest key in the table.
    ///
    /// This is always a fake node with key '' and has an entry in every single
    /// level. The code will NEVER make any comparisons against the key/value in
    /// this node.
    first_node: Arc<Node>,

    writer_lock: AsyncMutex<WriterState>,
}

struct WriterState {
    rng: MersenneTwisterRng,
}

struct Node {
    key: Bytes,
    value: Bytes,

    /// Next node in each level.
    /// - next_nodes[0] is the lowest level (contains all nodes).
    /// - next_nodes[len-1] is the highest level (has few nodes)
    ///
    /// TODO: Use AtomicArcs here to avoid any locking requirements.
    next_nodes: Vec<AtomicArc<Node>>,
}

impl SkipListMemtable {
    pub fn new(comparator: Arc<dyn KeyComparator>) -> Self {
        Self {
            comparator,
            first_node: Arc::new(Node {
                key: Bytes::new(),
                value: Bytes::new(),
                next_nodes: vec![AtomicArc::default(); HEIGHT],
            }),
            writer_lock: AsyncMutex::new(WriterState {
                rng: MersenneTwisterRng::mt19937(),
            }),
        }
    }

    pub async fn insert(&self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
        // Correctness requires that only one thread is inserting at a time.
        let mut writer_lock = self.writer_lock.lock().await?.enter();

        // Randomly pick which level should be used for the new node.
        let new_level = {
            let num = writer_lock.rng.next_u32();

            // Unlikely for 'level' to be a large number.
            // The '/ 2' gives us a 4x branching factor per level.
            let level = num.leading_zeros() / 2;

            core::cmp::min(level as usize, HEIGHT - 1)
        };

        // TODO: Need arena allocation for keys.
        let key: Bytes = key.into();
        let value: Bytes = value.into();

        // NOTE: These are in order of highest to lowest level index.
        let mut prev_nodes = vec![];
        prev_nodes.reserve_exact(new_level + 1);

        let mut next_nodes = vec![];
        next_nodes.reserve_exact(new_level + 1);
        next_nodes.resize_with(new_level + 1, || AtomicArc::default());

        // TODO: If we are inserting a batch of sorted keys, we should all be
        // efficienctly insertable using one memtable lock if we re-use the iterator
        // result for faster skip ahead.

        // TODO: Deduplicate this with find_previous_node
        let mut current_node = self.first_node.clone();
        let mut current_level = current_node.next_nodes.len() - 1;
        loop {
            // TODO: We can probably reduce the number of Arc clones that happen in this
            // loop.

            let next_node = current_node.next_nodes[current_level].clone();

            if let Some(next_node) = next_node.load() {
                if self.comparator.compare(&key[..], &next_node.key).is_ge() {
                    current_node = next_node;
                    continue;
                }
            }

            if current_level <= new_level {
                next_nodes[current_level] = next_node;
                prev_nodes.push(current_node.clone());
            }

            if current_level == 0 {
                break;
            }

            current_level -= 1;
        }

        // TODO: Arena allocate nodes. We shouldn't even need to track them with Arcs
        // since they won't be de-allocated until the memtable is dropped.
        let new_node = Arc::new(Node {
            key,
            value,
            next_nodes,
        });

        for (i, node) in prev_nodes.into_iter().enumerate() {
            let level = new_level - i;
            // TODO: May be more efficient to perform a swap operation here (then we don't
            // need to pre-build next_nodes using extra Arc clones).
            node.next_nodes[level].store(Some(new_node.clone()));
        }

        writer_lock.exit();

        Ok(())
    }

    pub fn iter(&self) -> SkipListMemtableIterator {
        SkipListMemtableIterator {
            comparator: self.comparator.clone(),
            first_node: self.first_node.clone(),
            // TODO: Avoid this operation if we are going to immediately seek elsewhere.
            current_node: self.first_node.next_nodes[0].load(),
        }
    }

    /// Note that this operation is rarely used so there is no point in caching
    /// the value of the largest key.
    pub fn key_range(&self) -> Option<(Bytes, Bytes)> {
        let first_node = match self.first_node.next_nodes[0].load() {
            Some(v) => v,
            None => return None,
        };

        let (last_node, _) =
            Self::find_previous_node(b"", &self.first_node, &AlwaysGreaterComparator::default());

        Some((first_node.key.clone(), last_node.key.clone()))
    }

    /// Finds the last node after first_node that is <= 'key'.
    ///
    /// Note that this only searches in levels occupied by 'first_node'.
    ///
    /// Returns the found node and whether or not we found an exact match to
    /// 'key'. May return first_node is all remaining nodes are > 'key'.
    ///
    /// TODO: De-duplicate with the insert code path
    fn find_previous_node(
        key: &[u8],
        first_node: &Arc<Node>,
        comparator: &dyn KeyComparator,
    ) -> (Arc<Node>, bool) {
        let mut current_node = first_node.clone();
        let mut current_level = first_node.next_nodes.len() - 1;
        let mut found_exact = false;
        loop {
            let next_node = current_node.next_nodes[current_level].load();

            if let Some(next_node) = next_node {
                let cmp = comparator.compare(key, &next_node.key);
                if cmp.is_eq() {
                    found_exact = true;
                    current_node = next_node;
                    break;
                }

                if cmp.is_gt() {
                    current_node = next_node;
                    continue;
                }
            }

            if current_level == 0 {
                break;
            }

            current_level -= 1;
        }

        (current_node, found_exact)
    }
}

pub struct SkipListMemtableIterator {
    comparator: Arc<dyn KeyComparator>,
    first_node: Arc<Node>,
    current_node: Option<Arc<Node>>,
}

#[async_trait]
impl Iterable<KeyValueEntry> for SkipListMemtableIterator {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        let node = self.current_node.take();

        if let Some(node) = &node {
            self.current_node = node.next_nodes[0].load();
        }

        Ok(node.map(|n| KeyValueEntry {
            key: n.key.clone(),
            value: n.value.clone(),
        }))
    }

    async fn seek(&mut self, key: &[u8]) -> Result<()> {
        // TODO: If seeking to a key that is > current_node, then ideally use the full
        // set of previous node pointers as a hint.

        // Finding the last node that is <= key.
        let (current_node, found_exact) =
            SkipListMemtable::find_previous_node(key, &self.first_node, self.comparator.as_ref());

        if !found_exact {
            self.current_node = current_node.next_nodes[0].load();
        } else {
            self.current_node = Some(current_node);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::table::BytewiseComparator;

    use super::*;

    #[testcase]
    async fn works() -> Result<()> {
        let list = SkipListMemtable::new(Arc::new(BytewiseComparator::new()));

        let mut data = vec![];
        for i in 0..10000 {
            if i % 2 == 0 {
                data.push(format!("{:08}", i));
            }
        }

        let mut data2 = data.clone();
        crypto::random::clocked_rng().shuffle(&mut data2[..]);

        for s in data2 {
            list.insert(s.as_bytes().into(), vec![]).await?;
        }

        for i in 0..10000 {
            let key = format!("{:08}", i);

            let mut iter = list.iter();
            iter.seek(key.as_bytes()).await?;

            for j in i..core::cmp::min(10000, i + 100) {
                if j % 2 == 0 {
                    let key = format!("{:08}", j);
                    let value = iter.next().await?.unwrap();
                    assert_eq!(&value.key[..], key.as_bytes());
                }
            }
        }

        let range = list.key_range().unwrap();
        assert_eq!(&range.0, &b"00000000"[..]);
        assert_eq!(&range.1, &b"00009998"[..]);

        // TODO: Test seeking multiple times (either significantly forward or backward
        // compared to the current state of the iterator).
        // - Test short and long seeks (short meaning that can they can be reached
        //   quickly with only low level scanning).

        Ok(())
    }
}
