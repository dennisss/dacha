use std::collections::{HashMap, HashSet};
use std::ops::DerefMut;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use common::algorithms::lower_bound_by;
use common::async_std;
use common::async_std::channel;
use common::async_std::channel::TrySendError;
use common::async_std::fs::{File, OpenOptions};
use common::async_std::path::{Path, PathBuf};
use common::async_std::prelude::*;
use common::async_std::sync::{Mutex, RwLock};
use common::bytes::Bytes;
use common::errors::*;
use common::fs::sync::SyncedPath;
use common::hex;
use common::task::ChildTask;
use crypto::random::SharedRng;
use fs2::FileExt;

use crate::db::internal_key::*;
use crate::db::merge_iterator::MergeIterator;
use crate::db::options::*;
use crate::db::version::*;
use crate::db::version_edit::*;
use crate::db::write_batch::*;
use crate::iterable::*;
use crate::memtable::memtable::MemTable;
use crate::memtable::*;
use crate::record_log::*;
use crate::table::comparator::KeyComparator;
use crate::table::table::{SSTable, SSTableIterator, SSTableOpenOptions};
use crate::table::table_builder::{SSTableBuilder, SSTableBuilderOptions, SSTableBuiltMetadata};

use super::paths::FilePaths;
use super::snapshot::Snapshot;

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

/*
Abstracting away the log and internla key:
- Will still use a manifest for tracking files (but log numbers will always be zero)
*/

struct CompactionReceiver {
    state: Arc<std::sync::Mutex<CompactionState>>,
    receiver: channel::Receiver<()>,
}

struct CompactionState {
    // Table file numbers which we know are ok to delete as they are no longer
    // referenced in the latest version.
    pending_release_files: HashSet<u64>,

    released_files: HashSet<u64>,
}

impl CompactionReceiver {
    fn new() -> (
        CompactionReceiver,
        channel::Sender<()>,
        FileReleasedCallback,
    ) {
        let (sender, receiver) = channel::bounded(1);

        let state = Arc::new(std::sync::Mutex::new(CompactionState {
            pending_release_files: HashSet::new(),
            released_files: HashSet::new(),
        }));

        let release_callback = Self::make_release_callback(state.clone(), sender.clone());

        (
            CompactionReceiver { state, receiver },
            sender,
            release_callback,
        )
    }

    fn make_release_callback(
        state: Arc<std::sync::Mutex<CompactionState>>,
        compaction_notifier: channel::Sender<()>,
    ) -> FileReleasedCallback {
        Arc::new(move |file_number: u64| {
            let mut state = state.lock().unwrap();
            if state.pending_release_files.remove(&file_number) {
                state.released_files.insert(file_number);
                let _ = compaction_notifier.try_send(());
            }
        })
    }
}

/// Single-process key-value store implemented as a Log Structured Merge tree
/// of in-memory and on-disk tables. Compatible with the LevelDB/RocksDB format.
pub struct EmbeddedDB {
    lock_file: std::fs::File,
    identity: Option<String>,
    shared: Arc<EmbeddedDBShared>,

    compaction_thread: ChildTask,

    /// Notified the background compaction thread that
    compaction_notifier: channel::Sender<()>,
}

struct EmbeddedDBShared {
    options: Arc<EmbeddedDBOptions>,
    dir: FilePaths,
    state: RwLock<EmbeddedDBState>,
    flushed_channel: (channel::Sender<()>, channel::Receiver<()>),
}

struct EmbeddedDBState {
    /// If true, then the database is no longer available for reads and writes.
    /// In this state, background compaction threads should hurry up and finish
    /// what they are doing so that we can close the database gracefully.
    closing: bool,

    /// Current log being used to write new changes before they are compacted
    /// into a table.
    ///
    /// Will be None only if EmbeddedDBOptions.disable_wal was set.
    log: Option<RecordWriter>,

    /// Last sequence written to the log but not yet flushed to a table.
    log_last_sequence: u64,

    /// Primary table for reading/writing latest values.
    mutable_table: Arc<MemTable>,

    /// Immutable table currently being written to disk.
    immutable_table: Option<ImmutableTable>,

    /// Contains of the state persisted to disk up to now.
    version_set: VersionSet,

    /// User callbacks which are notified next time the compaction thread is
    /// idle and has no more work pending.
    compaction_callbacks: Vec<channel::Sender<()>>,
}

#[derive(Clone)]
struct ImmutableTable {
    table: Arc<MemTable>,
    last_sequence: u64,
}

impl EmbeddedDB {
    /// Opens an existing database or creates a new one.
    pub async fn open<P: AsRef<Path>>(path: P, options: EmbeddedDBOptions) -> Result<Self> {
        Self::open_impl(path.as_ref(), options).await
    }

    async fn open_impl(path: &Path, options: EmbeddedDBOptions) -> Result<Self> {
        // TODO: Cache a file description to the data directory and use openat for
        // faster opens?

        let options = Arc::new(options.wrap_with_internal_keys());

        if options.create_if_missing {
            // TODO: Only create up to one directory.
            async_std::fs::create_dir_all(path).await?;
        }

        let dir = FilePaths::new(path)?;

        let lock_file = {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true)
                .create(options.create_if_missing)
                .read(true);

            let file = opts
                .open(dir.lock().read_path())
                .map_err(|_| err_msg("Failed to open the lockfile"))?;
            file.try_lock_exclusive()
                .map_err(|_| err_msg("Failed to lock database"))?;
            file
        };

        if let Some(manifest_path) = dir.current_manifest().await? {
            if options.error_if_exists {
                return Err(err_msg("Database already exists"));
            }

            Self::open_existing(options, lock_file, dir, manifest_path).await
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
        // TODO: In this mode, ensure that we truncate any existing files (they may have
        // been partially opened only).

        let (compaction_receiver, compaction_notifier, release_callback) =
            CompactionReceiver::new();

        let mut version_set = VersionSet::new(release_callback, options.clone());

        let manifest_num = version_set.next_file_number();
        {
            let mut edit = VersionEdit::default();
            edit.next_file_number = Some(manifest_num + 1);
            version_set.apply_new_edit(edit, vec![]);
        }

        let manifest_path = dir.manifest(manifest_num);

        let mut manifest = RecordWriter::open_with(manifest_path).await?;

        let log = {
            if options.disable_wal {
                None
            } else {
                let num = version_set.next_file_number();

                let mut edit = VersionEdit::default();
                edit.next_file_number = Some(num + 1);
                edit.log_number = Some(num);
                version_set.apply_new_edit(edit, vec![]);

                Some(RecordWriter::open_with(dir.log(num)).await?)
            }
        };

        version_set.write_to_new(&mut manifest).await?;
        manifest.flush().await?;

        let identity = Self::uuidv4().await;
        {
            let mut id_file = dir
                .identity()
                .open(OpenOptions::new().create(true).truncate(true).write(true))
                .await?;
            id_file.write_all(identity.as_bytes()).await?;
            id_file.flush_and_sync().await?;
        }

        dir.set_current_manifest(manifest_num).await?;

        let mutable_table = Arc::new(MemTable::new(options.table_options.comparator.clone()));

        let shared = Arc::new(EmbeddedDBShared {
            options,
            dir,
            flushed_channel: channel::bounded(1),
            state: RwLock::new(EmbeddedDBState {
                closing: false,
                log,
                log_last_sequence: version_set.last_sequence(),
                mutable_table,
                immutable_table: None,
                version_set,
                compaction_callbacks: vec![],
            }),
        });

        let compaction_thread = ChildTask::spawn(Self::compaction_thread(
            shared.clone(),
            manifest,
            compaction_receiver,
        ));

        Ok(Self {
            compaction_notifier,
            compaction_thread,
            lock_file,
            identity: Some(identity),
            shared,
        })
    }

    async fn open_existing(
        options: Arc<EmbeddedDBOptions>,
        lock_file: std::fs::File,
        dir: FilePaths,
        manifest_path: SyncedPath,
    ) -> Result<Self> {
        let (compaction_receiver, compaction_notifier, release_callback) =
            CompactionReceiver::new();

        let version_set = {
            let mut manifest_file = RecordReader::open(manifest_path.read_path()).await?;
            VersionSet::recover_existing(&mut manifest_file, release_callback, options.clone())
                .await?
        };

        version_set.open_all(&dir).await?;

        let manifest = RecordWriter::open_with(manifest_path).await?;

        let mut log_last_sequence = version_set.last_sequence();

        // TODO: Need to be resilient to the final record in the log being incomplete
        // (in this we can consider the write to have failed). This applies to both the
        // prev_log_number and the log_number although values with the prev_log_number
        // can only be tolerated if the log_number is not present (otherwise we should
        // truncate it to fix it).
        let mut immutable_table = None;
        if let Some(num) = version_set.prev_log_number() {
            if options.disable_wal {
                return Err(err_msg(
                    "Existing db has a prev_log_number in disable_wal mode.",
                ));
            }

            let mut log = RecordReader::open(dir.log(num).read_path()).await?;
            let mut table = MemTable::new(options.table_options.comparator.clone());
            WriteBatchIterator::read_table(&mut log, &mut table, &mut log_last_sequence).await?;
            immutable_table = Some(ImmutableTable {
                table: Arc::new(table),
                last_sequence: log_last_sequence,
            });
        }

        let log;
        let mutable_table;

        if options.disable_wal {
            if version_set.log_number().is_some() {
                return Err(err_msg("Existing db has a log in disable_wal mode"));
            }

            log = None;
            mutable_table = Arc::new(MemTable::new(options.table_options.comparator.clone()));
        } else {
            let log_path = dir.log(
                version_set
                    .log_number()
                    .ok_or_else(|| err_msg("Existing db has no log"))?,
            );

            mutable_table = {
                let mut log_reader = RecordReader::open(log_path.read_path()).await?;

                let mut table = MemTable::new(options.table_options.comparator.clone());
                WriteBatchIterator::read_table(&mut log_reader, &mut table, &mut log_last_sequence)
                    .await?;

                Arc::new(table)
            };

            log = Some(RecordWriter::open_with(log_path).await?);
        }

        // TODO: Exists may ignore errors such as permission errors.
        let identity_path = dir.identity();
        let identity = if common::async_std::path::Path::new(identity_path.read_path())
            .exists()
            .await
        {
            let data = async_std::fs::read_to_string(identity_path.read_path()).await?;
            Some(data)
        } else {
            None
        };

        // TODO: Look up all files in the directory and delete any not-referenced
        // by the current log.
        // ^ We should do this in the VersionSet recovery code

        // Schedule initial compaction check.
        let _ = compaction_notifier.try_send(());

        let shared = Arc::new(EmbeddedDBShared {
            options,
            dir,
            flushed_channel: channel::bounded(1),
            state: RwLock::new(EmbeddedDBState {
                closing: false,
                log,
                log_last_sequence,
                mutable_table,
                immutable_table,
                version_set,
                compaction_callbacks: vec![],
            }),
        });

        let compaction_thread = ChildTask::spawn(Self::compaction_thread(
            shared.clone(),
            manifest,
            compaction_receiver,
        ));

        Ok(Self {
            lock_file,
            identity,
            compaction_notifier,
            compaction_thread,
            shared,
        })
    }

    pub async fn close(self) -> Result<()> {
        // TODO: Closing should either block or fail if there still exists try
        // references to any internal memory.

        // TODO: Should stop new compactions from starting and wait for any existing
        // operations to finish.
        {
            let mut state = self.shared.state.write().await;
            state.closing = true;
        }

        let _ = self.compaction_notifier.try_send(());

        self.compaction_thread.join().await;

        Ok(())
    }

    /// Returns the sequence of the last write which has been flushed to a table
    /// (not to the WAL).
    ///
    /// NOTE: This is only guranteed to be correct for writes applied with this
    /// library (not guranteed to be compatible with LevelDB/RocksDB behavior).
    pub async fn last_flushed_sequence(&self) -> u64 {
        let state = self.shared.state.read().await;
        state.version_set.last_sequence()
    }

    /// Blocks until the database has been flushed past the last time
    /// wait_for_flush() was called.
    ///
    /// last_flushed_sequence() can be used to get a monotonic marker
    /// corresponding to last write that was flushed.
    pub async fn wait_for_flush(&self) {
        let _ = self.shared.flushed_channel.1.recv().await;
    }

    /// Blocks until there are no more scheduled compactions.
    /// Note that if the database still receives writes after this is called,
    /// then this function may never return.
    pub async fn wait_for_compaction(&self) -> Result<()> {
        let (sender, receiver) = channel::bounded(1);

        {
            let mut state = self.shared.state.write().await;
            state.compaction_callbacks.push(sender);
        }

        let _ = self.compaction_notifier.try_send(());

        receiver.recv().await?;
        Ok(())
    }

    async fn compaction_thread(
        shared: Arc<EmbeddedDBShared>,
        manifest: RecordWriter,
        receiver: CompactionReceiver,
    ) {
        if shared.options.read_only {
            return;
        }

        if let Err(e) = Self::compaction_thread_inner(shared, manifest, receiver).await {
            eprintln!("EmbeddedDB compaction error: {}", e);
            // TODO: Trigger server shutdown and halt all writes to the
            // memtable?
        }
    }

    async fn compaction_thread_inner(
        shared: Arc<EmbeddedDBShared>,
        mut manifest: RecordWriter,
        receiver: CompactionReceiver,
    ) -> Result<()> {
        let key_comparator = shared.options.table_options.comparator.clone();

        let mut made_progress = true;

        loop {
            if made_progress {
                // Whenever we make any progress in the previous iteration, we
                // will try a second time.
            } else if receiver.receiver.recv().await.is_err() {
                return Ok(());
            }

            {
                let mut nums_to_delete = vec![];
                {
                    let mut compaction_state = receiver.state.lock().unwrap();
                    for file_num in compaction_state.released_files.drain() {
                        nums_to_delete.push(file_num);
                    }
                }

                for file_num in nums_to_delete {
                    println!("Deleting table number {}", file_num);
                    let path = shared.dir.table(file_num);
                    common::async_std::fs::remove_file(path.read_path()).await?;
                }
            }

            let state = shared.state.read().await;

            if state.closing {
                return Ok(());
            }

            made_progress = true;

            // TODO: How do we gurantee that no one is still writing to the table?
            // TODO: Pre-allocate the entire memtable with contiguous memory so that it is
            // likely to cache hit.
            if state.mutable_table.size() >= shared.options.write_buffer_size
                && !state.immutable_table.is_some()
            {
                // TODO: Most of this doesn't need to happen in disable_wal mode.

                let mut version_edit = VersionEdit::default();

                if !shared.options.disable_wal {
                    let new_log_num = state.version_set.next_file_number();
                    version_edit.next_file_number = Some(new_log_num + 1);
                    version_edit.prev_log_number = state.version_set.log_number();
                    version_edit.log_number = Some(new_log_num);
                }

                drop(state);

                let new_log = match version_edit.log_number {
                    Some(num) => Some(RecordWriter::open_with(shared.dir.log(num)).await?),
                    None => None,
                };

                if !shared.options.disable_wal {
                    let mut out = vec![];
                    version_edit.serialize(&mut out)?;
                    manifest.append(&out).await?;
                    manifest.flush().await?;
                }

                let mut state = shared.state.write().await;

                let mut table = Arc::new(MemTable::new(key_comparator.clone()));
                std::mem::swap(&mut table, &mut state.mutable_table);
                state.immutable_table = Some(ImmutableTable {
                    table,
                    last_sequence: state.log_last_sequence,
                });

                if !shared.options.disable_wal {
                    state.version_set.apply_new_edit(version_edit, vec![]);
                    state.log = new_log;
                }

                continue;
            }

            if let Some(memtable) = &state.immutable_table {
                let memtable = memtable.clone();

                // TODO: In the case that this fails, just skip the whole compaction.
                let key_range = memtable
                    .table
                    .key_range()
                    .await
                    .ok_or_else(|| err_msg("Empty memtable beign compacted"))?;

                let selected_level = state.version_set.pick_memtable_level(KeyRangeRef {
                    smallest: &key_range.0,
                    largest: &key_range.1,
                });

                let mut version_edit = VersionEdit::default();
                version_edit.last_sequence = Some(memtable.last_sequence);
                version_edit.next_file_number = Some(state.version_set.next_file_number());

                // We want to discard the old log as it is no longer needed.
                // Will always be not-None here if we didn't disable the WAL.
                let old_log_number = state.version_set.prev_log_number();
                if old_log_number.is_some() {
                    version_edit.prev_log_number = Some(0);
                }

                // TODO:
                let target_file_size = state
                    .version_set
                    .target_file_size(selected_level.level as u32);

                // Release lock so that we don't block IO while compacting.
                drop(state);

                let new_tables = Self::build_tables_from_iterator(
                    &shared,
                    Box::new(memtable.table.iter()),
                    !selected_level.found_overlap,
                    &mut version_edit,
                    target_file_size,
                    selected_level.level as u32,
                )
                .await?;

                println!("MEMTABLE TO: {}", selected_level.level);
                for entry in &version_edit.new_files {
                    println!("- NEW: {}", entry.number);
                }

                let mut out = vec![];
                version_edit.serialize(&mut out)?;
                manifest.append(&out).await?;
                manifest.flush().await?;

                if let Some(num) = old_log_number {
                    common::async_std::fs::remove_file(shared.dir.log(num).read_path()).await?;
                }

                let mut state_guard = shared.state.write().await;

                state_guard.immutable_table = None;
                state_guard
                    .version_set
                    .apply_new_edit(version_edit, new_tables);

                drop(state_guard);

                let _ = shared.flushed_channel.0.try_send(());

                // NOTE: After this, we will check the mutable_table on
                // the next iteration to see if it can be flushed.

                continue;
            }

            // This handles all level i -> level j compactions.
            if let Some(compaction) = state.version_set.select_tables_to_compaction() {
                println!(
                    "COMPACTION: {} -> {}",
                    compaction.level, compaction.next_level
                );

                // TODO: Implement trivial compaction of just moving files from one level to the
                // next if there is no overlap in the new level.
                // Other reasons to not do trivial compaction:
                // - Want to clean up deleted records (or overriden ones).
                // - The file size if way smaller than the target file size of the new level and
                //   we think that combining the files would improve

                let mut iters: Vec<Box<dyn Iterable>> = vec![];

                let mut version_edit = VersionEdit::default();
                // Store the next file number (will be used to allocate file numbers later);
                version_edit.next_file_number = Some(state.version_set.next_file_number());

                // TODO: If this is not level 0, then we can optimize this with a LevelIterator.
                for entry in compaction.tables {
                    version_edit.deleted_files.push(DeletedFileEntry {
                        level: entry.entry.level,
                        number: entry.entry.number,
                    });

                    println!("- DELETE: {}", entry.entry.number);

                    let table_guard = entry.table.lock().await;
                    let table = table_guard.as_ref().unwrap();
                    iters.push(Box::new(table.iter()));
                }

                // TOOD: A few optimizations of this that we can do:
                // - Use LevelIterator so that we don't have to open multiple levels in the
                //   level at the same time.
                // - Use binary search to find the start of the overlapping tables and exit
                //   early once the overlapping is done.
                for entry in compaction.next_level_tables {
                    version_edit.deleted_files.push(DeletedFileEntry {
                        level: entry.entry.level,
                        number: entry.entry.number,
                    });

                    println!("- DELETE2: {}", entry.entry.number);

                    let table_guard = entry.table.lock().await;
                    let table = table_guard.as_ref().unwrap();
                    iters.push(Box::new(table.iter()));
                }

                let iterator = Box::new(MergeIterator::new(
                    shared.options.table_options.comparator.clone(),
                    iters,
                ));

                // TODO: This level may not exist yet, so this may do out of bounds.
                let target_file_size = state
                    .version_set
                    .target_file_size(compaction.next_level as u32);

                let remove_deleted = !compaction.found_overlap;

                drop(state);

                let new_tables = Self::build_tables_from_iterator(
                    shared.as_ref(),
                    iterator,
                    remove_deleted,
                    &mut version_edit,
                    target_file_size,
                    1,
                )
                .await?;

                let mut out = vec![];
                version_edit.serialize(&mut out)?;
                manifest.append(&out).await?;
                manifest.flush().await?;

                {
                    let mut compaction_state = receiver.state.lock().unwrap();
                    for file in &version_edit.deleted_files {
                        compaction_state.pending_release_files.insert(file.number);
                    }
                }

                // TODO: Re-lock and apply all of the version edits
                let mut state_guard = shared.state.write().await;

                state_guard
                    .version_set
                    .apply_new_edit(version_edit, new_tables);

                // TODO: May be able to delete some table files if all
                // references are done.

                continue;
            }

            // TODO: Also check the manifest size to see if we should switch manifests.

            if state.compaction_callbacks.len() > 0 {
                drop(state);
                let mut state = shared.state.write().await;
                while let Some(sender) = state.compaction_callbacks.pop() {
                    let _ = sender.try_send(());
                }
            }

            made_progress = false;
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

        Other things to consider:
        - doing Concurrent compactions
            -

        When do we need to re-check for compactions:
        - Whenever a new key is inserted
            - May cause the memtable to become too large.
        - Whenever a Version is dropped, we should check if there is a deleted table that now only has one reference.

        */
    }

    /// Writes an iterator to one or more contigious tables in a single level.
    ///
    /// Arguments:
    /// - remove_deleted: If true, keys that were deleted will be removed from
    ///   the resulting table.
    async fn build_tables_from_iterator(
        shared: &EmbeddedDBShared,
        mut iterator: Box<dyn Iterable>,
        remove_deleted: bool,
        version_edit: &mut VersionEdit,
        target_file_size: u64,
        level: u32,
    ) -> Result<Vec<SSTable>> {
        struct CurrentTable {
            builder: SSTableBuilder,
            first_key: Bytes,
            last_key: Bytes,
            number: u64,
        }

        let mut current_table = None;

        let mut last_user_key = None;

        while let Some(entry) = iterator.next().await? {
            let ikey = InternalKey::parse(&entry.key)?;
            // TODO: Re-use the entry.user_key reference.
            let user_key = entry.key.slice(0..ikey.user_key.len());

            // We will only store the value with the highest sequence per unique user key.
            if Some(&user_key) == last_user_key.as_ref() {
                continue;
            }

            last_user_key = Some(user_key.clone());
            if remove_deleted && ikey.typ == ValueType::Deletion {
                continue;
            }

            let mut table = match current_table.take() {
                Some(table) => table,
                None => {
                    let number = version_edit.next_file_number.unwrap();
                    version_edit.next_file_number = Some(number + 1);

                    let builder = SSTableBuilder::open_with(
                        shared.dir.table(number),
                        shared.options.table_options.clone(),
                    )
                    .await?;

                    CurrentTable {
                        builder,
                        first_key: entry.key.clone(),
                        last_key: entry.key.clone(),
                        number,
                    }
                }
            };

            table.builder.add(&entry.key, &entry.value).await?;
            table.last_key = entry.key;

            if table.builder.estimated_file_size() >= target_file_size {
                let meta = table.builder.finish().await?;

                version_edit.new_files.push(NewFileEntry {
                    level,
                    number: table.number,
                    file_size: meta.file_size,
                    smallest_key: table.first_key.to_vec(),
                    largest_key: table.last_key.to_vec(),
                    sequence_range: None,
                });
            } else {
                current_table = Some(table);
            }
        }

        // Flush the final table.
        if let Some(table) = current_table.take() {
            // TODO: Deduplicate with above.

            let meta = table.builder.finish().await?;

            version_edit.new_files.push(NewFileEntry {
                level,
                number: table.number,
                file_size: meta.file_size,
                smallest_key: table.first_key.to_vec(),
                largest_key: table.last_key.to_vec(),
                sequence_range: None,
            });
        }

        // Open all newly created tables.
        let mut new_tables = vec![];
        for entry in &version_edit.new_files {
            new_tables.push(
                SSTable::open(
                    shared.dir.table(entry.number).read_path(),
                    SSTableOpenOptions {
                        comparator: shared.options.table_options.comparator.clone(),
                        block_cache: shared.options.block_cache.clone(),
                    },
                )
                .await?,
            );
        }

        Ok(new_tables)
    }

    pub async fn snapshot(&self) -> Snapshot {
        let state = self.shared.state.read().await;

        // TODO: Make this an inline vector with up to 2 elements.
        let mut memtables = vec![state.mutable_table.clone()];

        if let Some(memtable) = &state.immutable_table {
            memtables.push(memtable.table.clone());
        }

        Snapshot {
            options: self.shared.options.clone(),
            last_sequence: state.log_last_sequence,
            memtables,
            version: state.version_set.latest_version().clone(),
        }
    }

    pub async fn get(&self, user_key: &[u8]) -> Result<Option<Bytes>> {
        /*
        TODO: Unique optimizations that we can perform with this:
        - Never attempt to read from disk if the key if in the memtable.
        - Also after we have read a key, don't immediately update the priority queue with the next value as we usually don't care.
        - If we seek beyond the user's key, stop early (we don't care what the next entry is then.)
        */

        let snapshot = self.snapshot().await;
        let mut iter = snapshot.iter().await;
        iter.seek(user_key).await?;

        // TODO: Must ignore any values > the sequence of the snapshot.

        if let Some(entry) = iter.next().await? {
            // TODO: Use the user_key comparator (although I guess exact equality should
            // lalways have the same definition)?
            if entry.key == user_key {
                return Ok(Some(entry.value));
            }
        }

        Ok(None)
    }

    /// Applies a set of writes to the database in one atomic operation. This
    /// will return once the writes have been persisted to the WAL.
    ///
    /// TODO: If anything in here fails, they we need to mark the database as
    /// being in an error state and we can't allow reads or writes from the
    /// database at that point.
    ///
    /// TODO: Also note that we shouldn't increment the last_sequence until the
    /// write is successful.
    pub async fn write(&self, batch: &mut WriteBatch) -> Result<()> {
        if self.shared.options.read_only {
            return Err(err_msg("Database opened as read only"));
        }

        if batch.count() == 0 {
            return Err(err_msg("Writing an empty batch"));
        }

        // NOTE: We currently MUST acquire a write log to ensure that there aren't any
        // concurrent writes to the immutable memtable.
        let mut state = self.shared.state.write().await;

        if batch.sequence() != 0 {
            if batch.sequence() <= state.log_last_sequence {
                return Err(err_msg("Batch has custom non-monotonic sequence"));
            }
        } else {
            batch.set_sequence(state.log_last_sequence + 1);
        }

        state.log_last_sequence = batch.sequence();

        // TODO: Reads should still be allowed while this is occuring.
        if let Some(log) = &mut state.log {
            log.append(&batch.as_bytes()).await?;
            log.flush().await?;
        }

        batch.iter()?.apply(&state.mutable_table).await?;

        // TODO: Dedup this logic with above.
        let should_compact = state.mutable_table.size() >= self.shared.options.write_buffer_size
            && !state.immutable_table.is_some();

        drop(state);

        if should_compact && !self.shared.options.manual_compactions_only {
            let _ = self.compaction_notifier.try_send(());
        }

        Ok(())
    }

    pub async fn set(&self, key: &[u8], value: &[u8]) -> Result<()> {
        let mut batch = WriteBatch::new();
        batch.put(key, value);

        self.write(&mut batch).await
    }

    pub async fn delete(&self, key: &[u8]) -> Result<()> {
        let mut batch = WriteBatch::new();
        batch.delete(key);
        self.write(&mut batch).await
    }
}
