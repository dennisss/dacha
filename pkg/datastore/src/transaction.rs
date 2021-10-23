use std::collections::BTreeMap;

use common::bytes::Bytes;
use sstable::db::Snapshot;
use sstable::iterable::{Iterable, KeyValueEntry};
use sstable::memtable::VecMemTable;

// Having multiple iterators on a single transaction will not be supported.
// - At least not for iterating while

enum Operation {
    Delete,
    Put(Bytes),
}

struct Transaction {
    read_snapshot: Snapshot,

    /// NOTE: for a single transaction, this need not support concurrent
    /// insertions.
    pub(crate) writes: BTreeMap<Bytes, Operation>,
}

impl Transaction {
    pub fn put(&mut self, key: &[u8], value: &[u8]) {
        self.writes.insert(key.into(), Operation::Put(value.into()));
    }

    pub fn delete(&mut self, key: &[u8]) {
        self.writes.insert(key.into(), Operation::Delete);
    }

    pub fn iter<'a>(&'a self) -> TransactionIterator<'a> {
        // Merge the iterators with priority for the 'writes' if the keys match

        // Noteably must also support deletions

        // The simple solution would be to wrap in the same internal key format.

        // One issue being that the VecMemtable doesn't support deletions

        // But we can do BTreeMap<Bytes, Operation>

        todo!()
    }
}

pub struct TransactionIterator<'a> {
    writer_iter: std::collections::btree_map::Iter<'a, Bytes, Operation>,
    writer_iter_next: Option<Operation>,

    snapshot_iter: Box<dyn Iterable>,
    snapshot_iter_next: Option<KeyValueEntry>,
    // snapshot_iter_next: Option<>
}

/*
Rather than a key comparator, we need some function for comparing entries.
*/
