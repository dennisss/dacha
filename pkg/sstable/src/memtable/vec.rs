use std::sync::Arc;

use common::async_std::sync::RwLock;
use common::bytes::Bytes;

use crate::table::comparator::Comparator;

/// Very simple memory table implementation based on a simple sorted vector.
///
/// Concurrent readers are supported. At most one write can occur at a time.
///
/// Insertion Time:
/// - Worst case: O(N + log N)
/// - Best case: O(1) if new keys are larger than all previous keys.
///
/// Lookup Time:
/// - O(log N)
///
/// Iterator Next()
/// - Worst Case: O(log N) if the table has changed since the iterator was last
///   queried.
/// - Best Case: O(1)
pub struct VecMemTable {
    state: Arc<VecMemTableState>,
}

struct VecMemTableState {
    comparator: Arc<dyn Comparator>,
    data: RwLock<Vec<VecMemTableEntry>>,
}

#[derive(Clone)]
pub struct VecMemTableEntry {
    pub key: Bytes,
    pub value: Bytes,
}

impl VecMemTable {
    pub fn new(comparator: Arc<dyn Comparator>) -> Self {
        Self {
            state: Arc::new(VecMemTableState {
                comparator,
                data: RwLock::new(vec![]),
            }),
        }
    }

    pub async fn insert(&self, key: Vec<u8>, value: Vec<u8>) {
        let mut data = self.state.data.write().await;

        let index = common::algorithms::lower_bound_by(data.as_ref(), &key[..], |entry, key| {
            self.state.comparator.compare(&entry.key, key).is_ge()
        })
        .unwrap_or(data.len());

        data.insert(
            index,
            VecMemTableEntry {
                key: key.into(),
                value: value.into(),
            },
        );
    }

    pub fn iter(&self) -> VecMemTableIterator {
        VecMemTableIterator {
            table_state: self.state.clone(),
            table_len: 0,
            last_index: 0,
            last_key: None,
            seeking: false,
        }
    }
}

pub struct VecMemTableIterator {
    table_state: Arc<VecMemTableState>,

    /// Number of elements in the table when this iterator was last modified.
    table_len: usize,

    /// Index into the table's vector at which the last value returned from this
    /// iterator was retrieved.
    last_index: usize,

    /// The last key which we returned from the iterator (or the next key we
    /// want to see if 'seeking' is true).
    /// If none, then the iterator hasn't been initialized yet.
    last_key: Option<Bytes>,

    /// If true, then we just seeked at the 'last_key'.
    seeking: bool,
}

impl VecMemTableIterator {
    pub async fn next(&mut self) -> Option<VecMemTableEntry> {
        let data = self.table_state.data.read().await;
        if data.is_empty() {
            return None;
        }

        let last_key = match &self.last_key {
            Some(key) => key.as_ref(),
            None => {
                // In this case the iterator has never been called before so initialize it to
                // the first element of the array.
                self.last_key = Some(data[0].key.clone());
                self.last_index = 0;
                return Some(data[0].clone());
            }
        };

        let next_index = {
            if self.table_len == data.len() && !self.seeking {
                self.last_index + 1
            } else {
                let lower_i = self.last_index;
                let upper_i = lower_i + (data.len() - self.table_len);

                let mut i = common::algorithms::lower_bound_by(
                    &data[lower_i..upper_i],
                    last_key,
                    |entry, key| {
                        self.table_state
                            .comparator
                            .compare(entry.key.as_ref(), key)
                            .is_ge()
                    },
                )
                .unwrap_or(upper_i - lower_i);
                i += lower_i;

                // Unless we were seeking to an unknown key, then we know for sure that we
                // previously saw last_key in the array. Because we never delete entries, we
                // know that data[i] == last_key so we'll increment to get the next position
                // after the last returned key.
                if !self.seeking {
                    i += 1;
                }

                i
            }
        };

        if next_index >= data.len() {
            return None;
        }

        self.table_len = data.len();
        self.last_index = next_index;
        self.last_key = Some(data[next_index].key.clone());
        self.seeking = false;

        Some(data[next_index].clone())
    }

    pub fn seek(&mut self, key: &[u8]) {
        self.last_key = Some(key.into());
        self.last_index = 0;
        self.table_len = 0;
        self.seeking = true;
    }
}
