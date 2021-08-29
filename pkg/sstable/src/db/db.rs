use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use common::algorithms::lower_bound_by;
use common::async_std;
use common::async_std::channel;
use common::async_std::fs::{File, OpenOptions};
use common::async_std::sync::{Mutex, RwLock};
use common::bytes::Bytes;
use common::errors::*;
use common::hex;
use crypto::random::SharedRng;
use fs2::FileExt;

use crate::db::internal_key::*;
use crate::db::options::*;
use crate::db::version::*;
use crate::db::version_edit::*;
use crate::db::write_batch::Write::Value;
use crate::db::write_batch::*;
use crate::memtable::memtable::MemTable;
use crate::memtable::*;
use crate::record_log::*;
use crate::table::comparator::Comparator;
use crate::table::table::{SSTable, SSTableIterator};
use crate::table::table_builder::{SSTableBuilder, SSTableBuilderOptions};

use super::paths::FilePaths;

// TODO: See https://github.com/google/leveldb/blob/c784d63b931d07895833fb80185b10d44ad63cce/db/filename.cc#L78 for all owned files

/*

LevelDB terminology:
- A 'Version' of the database is immutable set of of files which make up the database on disk
    - All iterators are defined at a single 'Version'
    - No file in a 'Version' can be deleted until the iterator is deleted.
- Simarly the memtable will be ref-counted so that iterators can have a consistent view of everything.

NOTE: Also check when restoring the log if the memtable can be immediately flushed to disk.

- When a memtable is compacted, it is pushed to the highest most layer while there is no overlapping tables
    - https://github.com/google/leveldb/blob/13e3c4efc66b8d7317c7648766a930b5d7e48aa7/db/version_set.cc#L472
*/

/*
    Flushing a table to disk:
    - Make the mutable_table immutable (and simulataneously swap to a new log file).
    - Create a new SSTable on disk
    - Write a new version of the MANIFEST pointing to an empty log file
*/
// TODO: Before deleting all un-used files, be sure to use absolute paths.

// TODO: Should implement read/write options like: https://github.com/google/leveldb/blob/9bd23c767601a2420478eec158927882b879bada/include/leveldb/options.h#L146

/*
Challenges: Can't delete an old file until it is deleted.

*/

/// Single-process key-value store implemented as a Log Structured Merge tree
/// of in-memory and on-disk tables. Compatible with the LevelDB/RocksDB format.
pub struct EmbeddedDB {
    options: Arc<EmbeddedDBOptions>,
    dir: FilePaths,
    lock_file: std::fs::File,
    identity: Option<String>,
    state: RwLock<EmbeddedDBState>,

    /// Notified the background compaction thread that
    compaction_notifier: channel::Sender<()>,
}

struct EmbeddedDBState {
    log: RecordWriter,

    manifest: RecordWriter,

    /// Primary table for reading/writing latest values.
    mutable_table: MemTable,

    /// Immutable table currently being written to disk.
    immutable_table: Option<MemTable>,

    /// Contains of the state persisted to disk ap
    version_set: VersionSet,
}

impl EmbeddedDB {
    /// Opens an existing database or creates a new one.
    pub async fn open(path: &Path, options: EmbeddedDBOptions) -> Result<Self> {
        // TODO: Cache a file description to the data directory and use openat for
        // faster opens?

        let options = Arc::new(options.wrap_with_internal_keys());

        if options.create_if_missing {
            async_std::fs::create_dir_all(path).await?;
        }

        let dir = FilePaths::new(path.to_owned());

        let lock_file = {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true)
                .create(options.create_if_missing)
                .read(true);

            let file = opts
                .open(dir.lock())
                .map_err(|_| err_msg("Failed to open the lockfile"))?;
            file.try_lock_exclusive()
                .map_err(|_| err_msg("Failed to lock database"))?;
            file
        };

        let current_path = dir.current();

        // TODO: Exists may ignore errors such as permission errors.
        if common::async_std::path::Path::new(&current_path)
            .exists()
            .await
        {
            if options.error_if_exists {
                return Err(err_msg("Database already exists"));
            }

            Self::open_existing(options, lock_file, dir).await
        } else {
            if !options.create_if_missing {
                return Err(err_msg("Database doesn't exist"));
            }

            Self::open_new(options, lock_file, dir).await
        }
    }

    async fn uuidv4() -> String {
        let mut data = vec![0u8; 16];
        crypto::random::global_rng().generate_bytes(&mut data).await;

        format!(
            "{}-{}-{}-{}-{}",
            hex::encode(&data[0..4]),
            hex::encode(&data[4..6]),
            hex::encode(&data[6..8]),
            hex::encode(&data[8..10]),
            hex::encode(&data[10..])
        )
    }

    async fn open_new(
        options: Arc<EmbeddedDBOptions>,
        lock_file: std::fs::File,
        dir: FilePaths,
    ) -> Result<Self> {
        let mut version_set = VersionSet::new(options.clone());

        let manifest_path = {
            let manifest_num = version_set.next_file_number;
            version_set.next_file_number += 1;
            dir.manifest(manifest_num)
        };

        let mut manifest = RecordWriter::open(&manifest_path).await?;

        let log = {
            let num = version_set.next_file_number;
            version_set.next_file_number += 1;
            RecordWriter::open(&dir.log(num)).await?
        };

        version_set.write_to_new(&mut manifest).await?;

        let identity = Self::uuidv4().await;
        common::async_std::fs::write(&dir.identity(), &identity).await?;

        common::async_std::fs::write(
            &dir.current(),
            format!("{}\n", manifest_path.file_name().unwrap().to_str().unwrap()),
        )
        .await?;

        let mutable_table = MemTable::new(options.table_options.comparator.clone());

        Ok(Self {
            options,
            dir,
            lock_file,
            identity: Some(identity),
            state: RwLock::new(EmbeddedDBState {
                log,
                manifest,
                mutable_table,
                immutable_table: None,
                version_set,
            }),
        })
    }

    async fn open_existing(
        options: Arc<EmbeddedDBOptions>,
        lock_file: std::fs::File,
        dir: FilePaths,
    ) -> Result<Self> {
        let mut current = async_std::fs::read_to_string(&dir.current()).await?;
        current = current.trim_end().to_string();

        let manifest_path = dir.root_dir().join(&current);

        let mut version_set = {
            let mut manifest_file = RecordReader::open(&manifest_path).await?;
            VersionSet::recover_existing(&mut manifest_file, options.clone()).await?
        };

        version_set.latest_version.open_all(&dir).await?;

        let manifest = RecordWriter::open(&manifest_path).await?;

        let mut immutable_table = None;
        if let Some(num) = version_set.prev_log_number {
            let mut log = RecordReader::open(&dir.log(num)).await?;
            let mut table = MemTable::new(options.table_options.comparator.clone());
            WriteBatchIterator::read_table(&mut log, &mut table, &mut version_set.last_sequence)
                .await?;
            immutable_table = Some(table);
        }

        let log_path = dir.log(
            version_set
                .log_number
                .ok_or_else(|| err_msg("Existing db has no log"))?,
        );

        let mutable_table = {
            let mut log_reader = RecordReader::open(&log_path).await?;

            let mut table = MemTable::new(options.table_options.comparator.clone());
            WriteBatchIterator::read_table(
                &mut log_reader,
                &mut table,
                &mut version_set.last_sequence,
            )
            .await?;

            table
        };

        let log = RecordWriter::open(&log_path).await?;

        // TODO: Exists may ignore errors such as permission errors.
        let identity_path = dir.identity();
        let identity = if common::async_std::path::Path::new(&identity_path)
            .exists()
            .await
        {
            let data = async_std::fs::read_to_string(identity_path).await?;
            Some(data)
        } else {
            None
        };

        // TODO: Look up all files in the directory and delete any not-referenced
        // by the current log.
        // ^ We should do this in the VersionSet recovery code

        let (sender, receiver) = channel::bounded(1);

        // Schedule initial compaction check.
        sender.try_send(());

        Ok(Self {
            compaction_notifier: sender,
            options,
            dir,
            lock_file,
            identity,
            state: RwLock::new(EmbeddedDBState {
                manifest,
                log,
                mutable_table,
                immutable_table,
                version_set,
            }),
        })
    }

    async fn background_thread(self: Arc<Self>, receiver: channel::Receiver<()>) -> Result<()> {
        loop {
            receiver.recv().await; // TODO: Handle return value.

            let mut state = self.state.write().await;

            // TODO: Pre-allocate the entire memtable with contiguous memory so that it is
            // likely to cache hit.
            if state.mutable_table.size() >= self.options.write_buffer_size
                && !state.immutable_table.is_some()
            {
                let mut table = MemTable::new(self.options.table_options.comparator.clone());
                std::mem::swap(&mut table, &mut state.mutable_table);
                state.immutable_table = Some(table);

                let new_log_num = state.version_set.next_file_number;

                state.version_set.next_file_number += 1;
                state.version_set.log_number = Some(new_log_num);
                state.version_set.prev_log_number = state.version_set.log_number.clone();

                // TODO: Deduplicate with above
                let mut version_edit = VersionEdit::default();
                version_edit.next_file_number = Some(state.version_set.next_file_number);
                version_edit.log_number = Some(new_log_num);
                version_edit.prev_log_number = state.version_set.prev_log_number.clone();

                let mut out = vec![];
                version_edit.serialize(&mut out)?;
                state.manifest.append(&out).await?;

                state.log = RecordWriter::open(&self.dir.log(new_log_num)).await?;
            }

            if state.immutable_table.is_some() {
                let file_number = state.version_set.next_file_number;
                state.version_set.next_file_number += 1;

                // TODO: Release locks and write to disk

                // Then re-acquire locks
                // Find the best level for the table

                // TODO: After this, we want to re-check the mutable_table on
                // the next iteration to see if it can be flushed.

                // TODO: Delete the old log file.
            }
        }

        /*
        Things to do:
        - Check if mutable table is over its limit (and there is no immutable memtable).
            - Make it immutable and switch to a new log file.

        - Check if there is a prev_log_number/immutable_memtable
            - If so, flush to disk.

        - Check level 0 against max num files
            - If over the limit, merge into next level.

        - Go through each level.
            - If a level is over its limit, pick a random table in the level and merge into the next lower level
            - Try to pick enough contiguous tables to merge in order to build the

        - doing Concurrent compactions
            -


        */
    }

    async fn get_from_memtable(
        &self,
        memtable: &MemTable,
        user_key: &[u8],
        seek_ikey: &[u8],
    ) -> Option<Option<Bytes>> {
        // The first value should be the one with the highest value.
        let mut iter = memtable.range_from(&seek_ikey);

        if let Some(entry) = iter.next().await {
            let ikey = InternalKey::parse(&entry.key).unwrap();

            // TODO: Use user comparator.
            if ikey.user_key == user_key {
                if ikey.typ == ValueType::Deletion {
                    return Some(None);
                } else {
                    return Some(Some(entry.value));
                }
            }
        }

        None
    }

    pub async fn get(&self, user_key: &[u8]) -> Result<Option<Bytes>> {
        let seek_ikey = InternalKey::before(user_key).serialized();
        // let snapshot_sequence = 0xffffff; // TODO:

        let state = self.state.read().await;

        // Try both memtables.
        if let Some(result) = self
            .get_from_memtable(&state.mutable_table, user_key, &seek_ikey)
            .await
        {
            return Ok(result);
        }
        if let Some(table) = &state.immutable_table {
            if let Some(result) = self.get_from_memtable(table, user_key, &seek_ikey).await {
                return Ok(result);
            }
        }

        // NOTE: Don't need to write a log

        // Check for all tables that would overlap with the desired key.
        // TODO: Will need a 'contains_prefix' incase a single user key was
        // shared across multiple tables.

        // Otherwise find all relevant SSTables, clone the references, unlock

        // Then using the cloned table references, look up the key until we
        // find it.

        Ok(None)
    }

    pub async fn set(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let mut state = self.state.write().await;

        let sequence = state.version_set.last_sequence + 1;
        state.version_set.last_sequence = sequence;

        let mut write_batch = vec![];
        serialize_write_batch(sequence, &[Write::Value { key, value }], &mut write_batch);
        state.log.append(&write_batch).await?;
        // TODO: Need to flush the log.

        let ikey = InternalKey {
            user_key: key,
            sequence,
            typ: ValueType::Value,
        }
        .serialized();

        state.mutable_table.insert(ikey, value.to_vec()).await;

        Ok(())
    }

    // pub async fn delete(&self, key: &[u8]) -> Result<()> {}
}

struct EmbeddedDBIterator {
    // Step 1: reference the memtables:
    // Step 2: Have sstable iterators.
    // When an SSTable iterator is exhausted, go to the next index in each level.
    version: Arc<Version>,

    // TODO: Need a Mem-Table iterator (or at least a small reference)
    /// At each level, we need to know the file number for the purpose of
    /// seeking the next one. (Although we may need to
    levels: Vec<(usize, SSTableIterator)>,
}
