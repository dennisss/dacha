use std::sync::Arc;
use std::time::Duration;

use common::errors::*;
use executor::cancellation::AlreadyCancelledToken;
use executor::lock_async;
use executor::sync::AsyncMutex;
use executor::sync::AsyncRwLock;
use executor_multitask::impl_resource_passthrough;
use executor_multitask::ServiceResource;
use executor_multitask::ServiceResourceGroup;
use file::{LocalPath, LocalPathBuf};
use protobuf::{Message, StaticMessage};
use raft::atomic::BlobFile;
use sstable::db::SnapshotIterator;
use sstable::db::SnapshotIteratorOptions;
use sstable::db::{Backup, Snapshot, Write, WriteBatch};
use sstable::iterable::Iterable;
use sstable::{EmbeddedDB, EmbeddedDBOptions};

use crate::meta::state_machine_db::EmbeddedDBStateMachineDatabase;
use crate::meta::watchers::*;
use crate::proto::*;

use super::table_key::TableKey;

/*
TODO: Rather than having synced path logic, we can maybe just always force syncing when opening files
*/

#[derive(Defaultable, Clone)]
pub struct EmbeddedDBStateMachineOptions {
    #[default(Duration::from_secs(60 * 60))] // 1 hour
    pub history_ttl: Duration,

    /// NOTE: Options related to how the database is opened shouldn't be
    /// overriden.
    pub db: EmbeddedDBOptions,
}

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
    /// Root data directory containing the individual snapshot sub-directories.
    dir: LocalPathBuf,

    options: EmbeddedDBStateMachineOptions,

    /// File used to mark
    current: AsyncMutex<(Current, BlobFile)>,

    group: ServiceResourceGroup,

    db: Arc<EmbeddedDBStateMachineDatabase>,

    watchers: Watchers,
}

impl_resource_passthrough!(EmbeddedDBStateMachine, group);

impl EmbeddedDBStateMachine {
    pub async fn open(dir: &LocalPath, options: &EmbeddedDBStateMachineOptions) -> Result<Self> {
        // TODO: Add a LOCK file and ensure that all file I/Os require that the lock is
        // still held.
        // (can also remove the internal EmbeddedDB lock per snapshot)

        let mut current = Current::default();

        let current_file = {
            let builder = BlobFile::builder(&dir.join("CURRENT")).await?;
            if builder.exists().await? {
                // TODO: Verify that this cleans up any past intermediate state.
                let (file, data) = builder.open().await?;
                current = Current::parse(&data)?;
                file
            } else {
                current.set_current_snapshot(1u32);
                builder.create(&current.serialize()?).await?
            }
        };

        for file in file::read_dir(dir)? {
            let num_prefix = match file.name().strip_prefix("snapshot-") {
                Some(v) => v,
                None => continue,
            };

            let num = num_prefix.parse::<u32>()?;
            if num == current.current_snapshot() {
                continue;
            }

            let path = dir.join(file.name());
            eprintln!("Delete stale snapshot: {:?}", path);
            file::remove_dir_all(path).await?;
        }

        let db_path = dir.join(format!("snapshot-{:04}", current.current_snapshot()));
        let db = Self::open_db(&db_path, options).await?;

        let db = Arc::new(EmbeddedDBStateMachineDatabase::create(db).await);

        let group = ServiceResourceGroup::new("EmbeddedDBStateMachine");
        group.register_dependency(db.clone()).await;
        group
            .spawn_interruptable(
                "waterline_updater",
                Self::waterline_updater(db.clone(), options.history_ttl),
            )
            .await;

        Ok(Self {
            db,
            options: options.clone(),
            group,
            dir: dir.to_owned(),
            current: AsyncMutex::new((current, current_file)),
            watchers: Watchers::new(),
        })
    }

    async fn open_db(
        path: &LocalPath,
        options: &EmbeddedDBStateMachineOptions,
    ) -> Result<EmbeddedDB> {
        let mut db_options = options.db.clone();
        db_options.create_if_missing = true;
        db_options.error_if_exists = false;
        db_options.disable_wal = true;
        db_options.enable_compaction_waterline = true;

        let db = EmbeddedDB::open(path, db_options).await?;
        Ok(db)
    }

    /// Background thread which continously advances the compaction waterline in
    /// the database to ensure that old data versions are eventually cleaned up.
    ///
    /// CANCEL SAFE
    async fn waterline_updater(
        db: Arc<EmbeddedDBStateMachineDatabase>,
        history_ttl: Duration,
    ) -> Result<()> {
        loop {
            let seq = db.read().await?.snapshot().await.last_sequence();
            executor::sleep(history_ttl).await?;
            db.read().await?.update_compaction_waterline(seq)?;
        }
    }

    /// WARNING: All users of this must ensure that they are not attemping to
    /// read below the compaction waterline.
    ///
    /// CANCEL SAFE
    pub async fn snapshot(&self) -> Snapshot {
        self.db.read().await.unwrap().snapshot().await
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

        let db = self.db.read().await?;

        let mut write = WriteBatch::from_bytes(op)?;
        write.set_sequence(index.value());
        db.write(&write).await?;

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

    async fn last_applied(&self) -> raft::LogIndex {
        self.db.read().await.unwrap().last_sequence().await.into()
    }

    async fn last_flushed(&self) -> raft::LogIndex {
        self.db
            .read()
            .await
            .unwrap()
            .last_flushed_sequence()
            .await
            .into()
    }

    async fn wait_for_flush(&self) {
        let f = { self.db.read().await.unwrap().wait_for_flush() };
        // NOTE: Must not keep 'self.db' locked as that would permanently block getting
        // the writer lock in restore().
        f.await
    }

    async fn snapshot(&self) -> Result<Option<raft::StateMachineSnapshot>> {
        let backup = self.db.read().await?.backup().await?;
        let last_applied = backup.last_sequence().into();
        let approximate_size = backup.approximate_size().await?;

        let (mut writer, reader) = common::pipe::pipe();

        // TODO: If the reader is dropped, then we can cancel this.
        executor::spawn(async move {
            let res = backup.write_to(&mut writer).await;
            writer.close(res).await;
        });

        Ok(Some(raft::StateMachineSnapshot {
            data: Box::new(reader),
            last_applied,
            approximate_size,
        }))
    }

    async fn restore(&self, data: raft::StateMachineSnapshot) -> Result<bool> {
        let mut current = self.current.lock().await?.read_exclusive();

        // TODO: Validate the last_applied isn't regressing.

        let num = current.0.current_snapshot() + 1;
        // TODO: Deduplicate this.
        let path = self.dir.join(format!("snapshot-{:04}", num));

        file::create_dir(&path).await?;

        match Backup::read_from(data.data, &path).await {
            Ok(()) => {}
            Err(e) => {
                eprintln!("Failed to restore snapshot to {:?}. Error: {}", path, e);
                file::remove_dir_all(&path).await?;
                return Ok(false);
            }
        }

        let mut new_db = Self::open_db(&path, &self.options).await?;

        let old_number = current.0.current_snapshot();

        // Note that we must write this before swapping out the DB instance since we
        // don't want to pre-maturely advertise that we have an up to date state
        // machine if it isn't durably committed to disk. Though either way, the state
        // machine can't be mutated concurrently to this function since the current
        // raft::Server implication will not perform concurrent apply() and restore()
        // calls.
        lock_async!(current <= current.upgrade(), {
            current.0.set_current_snapshot(num);
            current.1.store(&current.0.serialize()?).await
        })?;

        self.db.swap_with(new_db).await?;

        let old_path = self.dir.join(format!("snapshot-{:04}", old_number));
        file::remove_dir_all(&old_path).await?;

        Ok(true)
    }
}
