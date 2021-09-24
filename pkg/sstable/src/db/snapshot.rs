use std::sync::Arc;

use common::bytes::Bytes;
use common::errors::*;

use crate::db::internal_key::InternalKey;
use crate::db::level_iterator::*;
use crate::db::version::Version;
use crate::iterable::{Iterable, KeyValueEntry};
use crate::memtable::memtable::MemTable;
use crate::EmbeddedDBOptions;

use super::merge_iterator::MergeIterator;

pub struct Snapshot {
    pub(crate) options: Arc<EmbeddedDBOptions>,
    pub(crate) last_sequence: u64,
    pub(crate) memtables: Vec<Arc<MemTable>>,
    pub(crate) version: Arc<Version>,
}

impl Snapshot {
    pub async fn iter(&self) -> SnapshotIterator {
        let mut iters: Vec<Box<dyn Iterable>> = vec![];
        for table in &self.memtables {
            // println!("SNAPSHOT MEMTABLE");
            iters.push(Box::new(table.iter()));
        }

        if self.version.levels.len() > 0 {
            // TODO: These tables can be ordered with a preference towards reading from the
            // last one.
            for entry in &self.version.levels[0].tables {
                let guard = entry.table.lock().await;
                let table = guard.as_ref().unwrap();
                iters.push(Box::new(table.iter()));

                // println!("SNAPSHOT LEVEL 0 : {}", entry.entry.number);
            }
        }

        for level_index in 1..self.version.levels.len() {
            // for entry in &self.version.levels[level_index].tables {
            //     println!("SNAPSHOT LEVEL {} : {}", level_index, entry.entry.number);
            // }

            iters.push(Box::new(LevelIterator::new(
                self.version.clone(),
                level_index,
                self.options.clone(),
            )));
        }

        SnapshotIterator {
            snapshot_last_sequence: self.last_sequence,
            inner: MergeIterator::new(self.options.table_options.comparator.clone(), iters),
            last_user_key: None,
        }
    }
}

pub struct SnapshotIterator {
    /// Highest sequence number present in the current snapshot. We won't return
    /// any values with a higher sequence than this.
    snapshot_last_sequence: u64,

    inner: MergeIterator,

    last_user_key: Option<Bytes>,
}

#[async_trait]
impl Iterable for SnapshotIterator {
    async fn next(&mut self) -> Result<Option<KeyValueEntry>> {
        while let Some(entry) = self.inner.next().await? {
            let ikey = InternalKey::parse(&entry.key)?;
            // TODO: Re-use the entry.key reference
            let user_key = entry.key.slice(0..ikey.user_key.len());

            if ikey.sequence > self.snapshot_last_sequence {
                continue;
            }

            if Some(&user_key) == self.last_user_key.as_ref() {
                continue;
            }

            self.last_user_key = Some(user_key.clone());
            if ikey.typ == crate::db::internal_key::ValueType::Deletion {
                continue;
            }

            return Ok(Some(KeyValueEntry {
                key: user_key,
                value: entry.value,
            }));
        }

        Ok(None)
    }

    async fn seek(&mut self, key: &[u8]) -> Result<()> {
        self.inner
            .seek(&InternalKey::before(key).serialized())
            .await?;
        self.last_user_key = None;
        Ok(())
    }
}
