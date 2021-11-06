use std::sync::Arc;

use common::async_std::path::{Path, PathBuf};
use common::async_std::sync::Mutex;
use common::errors::*;
use protobuf::Message;
use raft::atomic::BlobFile;
use sstable::{EmbeddedDB, EmbeddedDBOptions};

use crate::proto::meta::*;

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

        let db = EmbeddedDB::open(db_path, db_options).await?;

        Ok(Self {
            db,
            dir: dir.to_owned(),
            current: Mutex::new((current, current_file)),
        })
    }

    /// TODO: Don't expose this.
    /// Instead just expose creating snapshots.
    pub fn db(&self) -> &EmbeddedDB {
        &self.db
    }
}

#[async_trait]
impl raft::StateMachine<()> for EmbeddedDBStateMachine {
    async fn apply(&self, index: raft::LogIndex, op: &[u8]) -> Result<()> {
        // The operation should be a serialized WriteBatch
        // We just need to add the sequence to it and then apply it.

        let mut write = sstable::db::WriteBatch::from_bytes(op)?;
        write.set_sequence(index.value());
        self.db.write(&mut write).await?;
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
