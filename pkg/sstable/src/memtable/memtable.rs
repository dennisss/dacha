use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::ops::Bound;
use std::path::Path;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;

use crate::iterable::Iterable;
use crate::memtable::vec::*;
use crate::table::comparator::*;

/*
Internal table implementation needs to support:
- Iteration (starting at a given key or the beginning)
- Insertion

*/

/*
    Key operations:
    - Given an initial internal key, and a seek direction, look up

    - Ideally use
*/

// TODO: To deal with snapshots, we need to store all entries ordered by
// sequence which aren't on disk already. (but can be GC'ed when initializing
// the table from a log)
pub struct MemTable {
    table: VecMemTable,
    size: AtomicUsize,
}

impl MemTable {
    pub fn new(comparator: Arc<dyn KeyComparator>) -> Self {
        Self {
            table: VecMemTable::new(comparator),
            size: AtomicUsize::new(0),
        }
    }

    /// Returns the total number of bytes the keys and values of this memtable
    /// store in memory.
    pub fn size(&self) -> usize {
        self.size.load(std::sync::atomic::Ordering::Relaxed)
    }

    //	pub fn get<'a>(&'a self, key: &'a [u8]) -> Option<TableValue<'a>> {
    //		let entry = match self.table.guard().inner().get(
    //			&MemTableKey::view(key)) {
    //			Some(e) => e,
    //			None => { return None; }
    //		};
    //
    //		if entry.typ == ValueType::Deletion {
    //			return Some(TableValue { sequence: entry.sequence, value: None });
    //		}
    //
    //		Some(TableValue { sequence: entry.sequence, value: Some(&entry.value) })
    //	}

    pub fn iter(&self) -> VecMemTableIterator {
        self.table.iter()
    }

    /// Creates an iterator over the memtable starting at the given key.
    pub fn range_from(&self, key: &[u8]) -> VecMemTableIterator {
        let mut iter = self.table.iter();
        iter.seek(key);
        iter
    }

    // TODO: Change to taking references as arguments as we eventually want to copy
    // the data into the memtable's arena.
    pub async fn insert(&self, key: Vec<u8>, value: Vec<u8>) {
        self.size.fetch_add(
            key.len() + value.len(),
            std::sync::atomic::Ordering::Relaxed,
        );
        self.table.insert(key, value).await;
    }

    pub async fn key_range(&self) -> Option<(Bytes, Bytes)> {
        self.table.key_range().await
    }
}
//
//pub struct TableValue<'a> {
//	pub sequence: u64,
//	/// If none, then this value was deleted
//	pub value: Option<&'a [u8]>
//}

//pub struct MemTableIterator<'a> {
//	current: (&'a [u8], &'a [u8])
//}

/*
    What can be done while iterating:
    -
*/
