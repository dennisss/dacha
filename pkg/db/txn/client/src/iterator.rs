// TODO: Finish this code. Ideally instead of reading out the entire response from the server on reads and then merging with local writes, do that all in one iterator.

use std::collections::{BTreeMap, VecDeque};
use std::ops::Bound;

use common::bytes::Bytes;
use common::errors::*;
use db_kv::*;
use executor::lock;
use executor::sync::AsyncMutex;


struct Iterator {
    /*
    Iterator<>
    */

    /// Iterator over the baseline set of values in the db.
    /// (values before applying the current transaction).
    iter: SnapshotIterator,

    iter_exhausted: bool,

    /// The next entry returned by 'iter' but not yet given back to the caller.
    next_iter_entry: Option<(Bytes, Bytes)>,

    /// Locally written changes that will override anything in 'iter'.
    local_entries: VecDeque<(Bytes, Operation)>,

    max_key: Bytes,
}

#[async_trait]
impl KeyValueStoreIterator for Iterator {
    // 2-way merging between the 'local_entries' and 'iter'
    // TODO: This code seriously needs some unit tests and could probably be generalized.
    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        while !self.local_entries.is_empty() && !self.iter_exhausted {
            // Get the next entry from 'iter'
            while !self.iter_exhausted && self.next_iter_entry.is_none() {
                let entry = match self.iter.next().await? {
                    Some(v) => v,
                    None => {
                        self.iter_exhausted = true;
                        break;
                    }
                };

                if &entry.key >= &self.max_key {
                    self.iter_exhausted = true;
                    break;
                }

                // Filter deletions
                let value = match entry.value {
                    Some(v) => v,
                    None => continue,
                };

                self.next_iter_entry = Some((entry.key, value));
            }

            let mut pick_iter = true;
            let mut pick_local = true;
    
            // Merge if both iter and local_entries have an available value.
            if let Some((iter_key, _)) = &self.next_iter_entry {
                if let Some((local_key, _)) = self.local_entries.front() {
                    let c = local_key.cmp(iter_key);
                    if c.is_eq() {
                        // Skip the iter entry since the local write overrides it.
                        self.next_iter_entry = None;
                    } else if c.is_gt() {
                        // 'iter' one is smaller
                        pick_local = false;
                        pick_iter = true;
                    } else {
                        // 'local' one is smaller
                        pick_local = true;
                        pick_iter = false;
                    }    
                }
            }
    
            if pick_local {
                if let Some((key, op)) = self.local_entries.pop_front() {
                    let value = match op {
                        Operation::Put(v) => v,
                        Operation::Delete => {
                            continue
                        }
                    };

                    return Ok(Some(KeyValueEntry {
                        key,
                        value,
                    }));
                }    
            }
    
            if pick_iter {
                if let Some((key, value)) = self.next_iter_entry.take() {
                    return Ok(Some(KeyValueEntry {
                        key,
                        value,
                    }));
                }    
            }
        }

        Ok(None)
    }
}