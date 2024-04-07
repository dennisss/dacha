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

/// A read-only point in time view of an EmbeddedDB.
///
/// This enables consistent reading over the database while encountering new
/// writes. Note that while this object is alive, it will prevent garbage
/// collection or compaction of the underlying table data.
pub struct Snapshot {
    pub(crate) options: Arc<EmbeddedDBOptions>,

    /// Highest sequence number present in the current snapshot. We won't return
    /// any values with a higher sequence than this.
    pub(crate) last_sequence: u64,

    pub(crate) compaction_waterline: Option<u64>,

    pub(crate) memtables: Vec<Arc<MemTable>>,

    pub(crate) version: Arc<Version>,
}

impl Snapshot {
    pub async fn iter(&self) -> Result<SnapshotIterator> {
        self.iter_with_options(SnapshotIteratorOptions::default())
            .await
    }

    pub async fn iter_with_options(
        &self,
        mut options: SnapshotIteratorOptions,
    ) -> Result<SnapshotIterator> {
        if *options.last_sequence.get_or_insert(self.last_sequence) > self.last_sequence {
            return Err(err_msg(
                "Requested sequence is greater than the max sequence in this snapshot",
            ));
        }

        if options.last_sequence.unwrap() < self.compaction_waterline.unwrap_or(0) {
            return Err(err_msg(
                "Requested sequence is below the compaction waterline so may have been compacted.",
            ));
        }

        // TODO: Need checking against the compaction waterline (can also be used to set
        // a lower bound).
        if let Some(first_sequence) = &options.first_sequence {
            if *first_sequence > self.last_sequence {
                return Err(err_msg("first_sequence > last_sequence"));
            }
        }

        let mut iters: Vec<Box<dyn Iterable<KeyValueEntry>>> = vec![];
        for table in &self.memtables {
            // println!("SNAPSHOT MEMTABLE");
            iters.push(Box::new(table.iter()));
        }

        if self.version.levels.len() > 0 {
            // TODO: These tables can be ordered with a preference towards reading from the
            // last one.
            for entry in &self.version.levels[0].tables {
                let iter = entry.table().await.iter();
                iters.push(Box::new(iter));

                // println!("SNAPSHOT LEVEL 0 : {}", entry.entry.number);
            }
        }

        // TODO: Skip tables which don't include the sequence range desired.
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

        Ok(SnapshotIterator {
            options,
            inner: MergeIterator::new(self.options.table_options.comparator.clone(), iters),
            last_user_key: None,
        })
    }

    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    pub fn compaction_waterline(&self) -> Option<u64> {
        self.compaction_waterline
    }

    pub async fn get(&self, user_key: &[u8]) -> Result<Option<Bytes>> {
        if let Some(entry) = self.entry(user_key).await? {
            Ok(entry.value)
        } else {
            Ok(None)
        }
    }

    pub async fn entry(&self, user_key: &[u8]) -> Result<Option<SnapshotKeyValueEntry>> {
        /*
        TODO: If any bloom/hash filters available.
        ^ Basically going

        TODO: Unique optimizations that we can perform with this:
        - Never attempt to read from disk if the key if in the memtable.
        - Also after we have read a key, don't immediately update the priority queue with the next value as we usually don't care.
        - If we seek beyond the user's key, stop early (we don't care what the next entry is then.)
        */

        let mut iter = self.iter().await?;
        iter.seek(user_key).await?;

        if let Some(entry) = iter.next().await? {
            // TODO: Use the user_key comparator (although I guess exact equality should
            // lalways have the same definition)?
            if entry.key == user_key {
                return Ok(Some(entry));
            }
        }

        Ok(None)
    }
}

#[derive(Default, Clone)]
pub struct SnapshotIteratorOptions {
    /// If true, all versions of a key may are present in storage are returned.
    /// By default, only all the latest version of each key is returned.
    pub return_all_versions: bool,

    /// If set, do not return any values which have a sequence number >
    /// last_sequence.
    ///
    /// Defaults to the last sequence in the database at the point at which the
    /// snapshot was created.
    pub last_sequence: Option<u64>,

    /// If set, do not return any values which have a sequence number <
    /// first_sequence.
    pub first_sequence: Option<u64>,
}

#[derive(Debug)]
pub struct SnapshotKeyValueEntry {
    /// User key associated with this entry.
    pub key: Bytes,

    /// If none, then this key is deleted
    pub value: Option<Bytes>,

    pub sequence: u64,
}

pub struct SnapshotIterator {
    options: SnapshotIteratorOptions,

    inner: MergeIterator,

    last_user_key: Option<Bytes>,
}

#[async_trait]
impl Iterable<SnapshotKeyValueEntry> for SnapshotIterator {
    async fn next(&mut self) -> Result<Option<SnapshotKeyValueEntry>> {
        while let Some(entry) = self.inner.next().await? {
            let ikey = InternalKey::parse(&entry.key)?;
            // TODO: Re-use the entry.key reference
            let user_key = entry.key.slice(0..ikey.user_key.len());

            if let Some(last_sequence) = &self.options.last_sequence {
                if ikey.sequence > *last_sequence {
                    continue;
                }
            }

            if let Some(first_sequence) = &self.options.first_sequence {
                if ikey.sequence < *first_sequence {
                    continue;
                }
            }

            if !self.options.return_all_versions {
                if Some(&user_key) == self.last_user_key.as_ref() {
                    continue;
                }

                self.last_user_key = Some(user_key.clone());
            }

            let value = if ikey.typ == crate::db::internal_key::ValueType::Deletion {
                None
            } else {
                Some(entry.value)
            };

            return Ok(Some(SnapshotKeyValueEntry {
                key: user_key,
                value,
                sequence: ikey.sequence,
            }));
        }

        Ok(None)
    }

    async fn seek(&mut self, key: &[u8]) -> Result<()> {
        let inner_key = {
            if let Some(last_sequence) = self.options.last_sequence.clone() {
                InternalKey::before_with_sequence(key, last_sequence)
            } else {
                InternalKey::before(key)
            }
        };

        self.inner.seek(&inner_key.serialized()).await?;
        self.last_user_key = None;
        Ok(())
    }
}
