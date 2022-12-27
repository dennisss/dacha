use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;
use executor::sync::RwLock;

use crate::iterable::{Iterable, KeyValueEntry};
use crate::table::comparator::KeyComparator;

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
    comparator: Arc<dyn KeyComparator>,
    data: RwLock<Vec<KeyValueEntry>>,
}

impl VecMemTable {
    pub fn new(comparator: Arc<dyn KeyComparator>) -> Self {
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
            KeyValueEntry {
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

    pub async fn key_range(&self) -> Option<(Bytes, Bytes)> {
        let data = self.state.data.read().await;
        if data.is_empty() {
            return None;
        }

        Some((data[0].key.clone(), data[data.len() - 1].key.clone()))
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

#[async_trait]
impl Iterable<KeyValueEntry> for VecMemTableIterator {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        let data = self.table_state.data.read().await;
        if data.is_empty() {
            return Ok(None);
        }

        let last_key = match &self.last_key {
            Some(key) => key.as_ref(),
            None => {
                // In this case the iterator has never been called before so initialize it to
                // the first element of the array.
                self.last_key = Some(data[0].key.clone());
                self.last_index = 0;
                return Ok(Some(data[0].clone()));
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
            return Ok(None);
        }

        self.table_len = data.len();
        self.last_index = next_index;
        self.last_key = Some(data[next_index].key.clone());
        self.seeking = false;

        Ok(Some(data[next_index].clone()))
    }

    async fn seek(&mut self, key: &[u8]) -> Result<()> {
        self.last_key = Some(key.into());
        self.last_index = 0;
        self.table_len = 0;
        self.seeking = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use common::errors::*;

    use crate::{iterable::Iterable, table::comparator::BytewiseComparator};

    use super::VecMemTable;

    #[testcase]
    async fn vec_memtable_iterate_while_inserting() -> Result<()> {
        let table = VecMemTable::new(Arc::new(BytewiseComparator::new()));

        table.insert(vec![0, 10], vec![1]).await;
        table.insert(vec![0, 20], vec![2]).await;
        table.insert(vec![0, 30], vec![3]).await;
        table.insert(vec![0, 40], vec![4]).await;

        let mut iter = table.iter();

        let e1 = iter.next().await?.unwrap();
        assert_eq!(&e1.key[..], &[0, 10]);
        assert_eq!(&e1.value[..], &[1]);

        table.insert(vec![0, 15], vec![5]).await;

        let e = iter.next().await?.unwrap();
        assert_eq!(&e.key[..], &[0, 15]);
        assert_eq!(&e.value[..], &[5]);

        let e = iter.next().await?.unwrap();
        assert_eq!(&e.key[..], &[0, 20]);
        assert_eq!(&e.value[..], &[2]);

        let e = iter.next().await?.unwrap();
        assert_eq!(&e.key[..], &[0, 30]);
        assert_eq!(&e.value[..], &[3]);

        table.insert(vec![1, 0], vec![6]).await;

        let e = iter.next().await?.unwrap();
        assert_eq!(&e.key[..], &[0, 40]);
        assert_eq!(&e.value[..], &[4]);

        let e = iter.next().await?.unwrap();
        assert_eq!(&e.key[..], &[1, 0]);
        assert_eq!(&e.value[..], &[6]);

        assert!(iter.next().await?.is_none());

        Ok(())
    }
}
