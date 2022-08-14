use std::sync::Arc;

use common::async_std::path::{Path, PathBuf};
use common::async_std::sync::Mutex;
use common::errors::*;
use protobuf::{Message, StaticMessage};
use raft::atomic::BlobFile;
use sstable::db::{Snapshot, Write, WriteBatch};
use sstable::iterable::Iterable;
use sstable::{EmbeddedDB, EmbeddedDBOptions};

use crate::meta::watchers::*;
use crate::proto::key_value::{KeyValueEntry, WatchResponse};
use crate::proto::meta::*;

/*
Compaction strategy:
- There will be a special metastore key which contains the waterline value
    => It will be changed by executing a command on the state machine
- The state machine will reject any mutation whose read_index is < the waterline

Every 1 hour, the metastore background thread will try to find a log index which is more than 1 hour

Other complexities:
- If a key wasn't changed for a while, it may

*/

/// Key-value state machine based on the EmbeddedDB implementation.
///
/// - Each operation is a serialized EmbeddedDB WriteBatch.
/// - Each WriteBatch has its sequence set to the corresponding raft::LogIndex.
///   - This makes it straight forward for us to track the last applied LogIndex
///     based on the last applied sequence on the EmbeddedDB.
///
/// File system usage:
/// The state machine adds the following files to the directory in which it is
/// started:
/// - CURRENT : Contains a serialized 'Current' proto indicating the location of
///   the current snapshot.
/// - snapshot-000N : Directory storing a single EmbeddedDB instance's data.
///   - New writes are written in-place into the snapshot directory pointed to
///     by the CURRENT file.
///   - Under normal operation, there will only be 1 snapshot directory and we
///     don't switch to new snapshot directories.
///   - If the current server falls behind its peers, we may receive a new
///     'catch-up' snapshot via the StateMachine::restore() method. This will be
///     implemented by writing the new snapshot into a new snapshot directory
///     and later switching to that directory.
pub struct EmbeddedDBStateMachine {
    db: EmbeddedDB,

    /// Root data directory containing the individual snapshot sub-folders.
    dir: PathBuf,
    current: Mutex<(Current, BlobFile)>,

    watchers: Watchers,
}

impl EmbeddedDBStateMachine {
    pub async fn open(dir: &Path) -> Result<Self> {
        let mut current = Current::default();

        let current_file = {
            let builder = BlobFile::builder(&dir.join("CURRENT")).await?;
            if builder.exists().await {
                // TODO: Verify that this cleans up any past intermediate state.
                let (file, data) = builder.open().await?;
                current = Current::parse(&data)?;
                file
            } else {
                current.set_current_snapshot(1u32);
                builder.create(&current.serialize()?).await?
            }
        };

        let db_path = dir.join(format!("snapshot-{:04}", current.current_snapshot()));

        let mut db_options = EmbeddedDBOptions::default();
        db_options.create_if_missing = true;
        db_options.error_if_exists = false;
        db_options.disable_wal = true;
        db_options.initial_compaction_waterline = 1;

        let db = EmbeddedDB::open(db_path, db_options).await?;

        Ok(Self {
            db,
            dir: dir.to_owned(),
            current: Mutex::new((current, current_file)),
            watchers: Watchers::new(),
        })
    }

    pub async fn snapshot(&self) -> Snapshot {
        self.db.snapshot().await
    }

    pub fn watchers(&self) -> &Watchers {
        &self.watchers
    }
}

#[async_trait]
impl raft::StateMachine<()> for EmbeddedDBStateMachine {
    async fn apply(&self, index: raft::LogIndex, op: &[u8]) -> Result<()> {
        // The operation should be a serialized WriteBatch
        // We just need to add the sequence to it and then apply it.

        let mut write = WriteBatch::from_bytes(op)?;
        write.set_sequence(index.value());
        self.db.write(&mut write).await?;

        // Send the change to watchers.
        // TODO: This can be parrallelized with future writes.
        let mut change = WatchResponse::default();
        for res in write.iter()? {
            let write = res?;
            let mut entry = KeyValueEntry::default();
            entry.set_sequence(index.value());
            match write {
                Write::Deletion { key } => {
                    entry.set_key(key);
                    entry.set_deleted(true);
                }
                Write::Value { key, value } => {
                    entry.set_key(key);
                    entry.set_value(value);
                }
            }

            change.add_entries(entry);
        }
        self.watchers.broadcast(&change).await;

        Ok(())
    }

    async fn last_flushed(&self) -> raft::LogIndex {
        self.db.last_flushed_sequence().await.into()
    }

    async fn wait_for_flush(&self) {
        self.db.wait_for_flush().await
    }

    async fn snapshot(&self) -> Option<raft::StateMachineSnapshot> {
        todo!()
    }

    async fn restore(&self, data: raft::StateMachineSnapshot) -> Result<()> {
        todo!();
    }
}
