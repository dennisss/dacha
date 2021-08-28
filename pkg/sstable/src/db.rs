use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use common::algorithms::lower_bound_by;
use common::async_std;
use common::async_std::fs::{File, OpenOptions};
use common::async_std::sync::{Mutex, RwLock};
use common::bytes::Bytes;
use common::errors::*;
use fs2::FileExt;

use crate::internal_key::*;
use crate::manifest::*;
use crate::memtable::memtable::MemTable;
use crate::memtable::*;
use crate::record_log::*;
use crate::table::comparator::Comparator;
use crate::table::table::{SSTable, SSTableIterator};
use crate::table::table_builder::{SSTableBuilder, SSTableBuilderOptions};
use crate::write_batch::Write::Value;
use crate::write_batch::*;

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

pub struct VersionSet {
    pub latest_version: Arc<Version>,
}

pub struct Version {
    /// All tables stored on disk.
    /// level_tables[i] corresponds to all tables in the i'th level.
    ///
    /// All level vectors other than level_tables[0] are in sorted order by
    /// smallest key and are non-overlapping in key ranges.
    pub levels: Vec<Level>,
}

pub struct Level {
    pub number: usize,

    /// Total size in bytes of all tables in this level
    pub total_size: u64,

    /// Maximum number of bytes allowed to be in this level until we
    pub max_size: u64,

    pub target_file_size: u64,

    pub tables: Vec<Arc<LevelTableEntry>>,
}

impl Version {
    pub fn insert(&mut self, new_file_entry: NewFileEntry) -> bool {}

    /// Open all not currently opened tables in this version.
    pub async fn open_all(&mut self) -> Result<()> {}
}

pub struct LevelTableEntry {
    /// Opened table reference. May be None if we have too many files open and
    /// had to close a table.
    pub table: Mutex<Option<Arc<SSTable>>>,

    pub entry: NewFileEntry,
}

impl Version {}

// Even with level 0, we can order it, btu

/// This is a level in which tables are sorted from oldest to newest where newer
/// tables may override keys present in older tables.
pub struct OverlayingLevel {
    tables: Vec<TableEntry>,
}

pub struct SegmentedLevel {}

pub struct OrderedVec {
    inner: Vec<NewFileEntry>,
}

impl OrderedVec {
    /// Returns false if the entry already exists.
    #[must_use]
    pub fn insert(&mut self, entry: NewFileEntry, key_comparator: &dyn Comparator) -> bool {
        let idx = lower_bound_by(self.inner.as_ref(), &entry, |e1, e2| {
            e1.compare(e2, key_comparator).is_ge()
        })
        .unwrap_or(self.inner.len());

        if idx < self.inner.len() && self.inner[idx].compare(&entry, key_comparator).is_eq() {
            return false;
        }

        self.inner.insert(idx, entry);
        true
    }

    /// Returns false if the entry wasn't found.
    #[must_use]
    pub fn remove(&mut self, file_number: u64) -> bool {
        for i in 0..self.inner.len() {
            if self.inner[i].number == file_number {
                self.inner.remove(i);
                return true;
            }
        }

        false
    }
}

/// Single-process key-value store implemented as a Log Structured Merge tree
/// of in-memory and on-disk tables. Compatible with the LevelDB/RocksDB format.
pub struct EmbeddedDB {
    options: EmbeddedDBOptions,
    dir: FilePaths,
    lock_file: std::fs::File,
    identity: Option<Vec<u8>>,
    next_file_number: AtomicU64,
    state: RwLock<EmbeddedDBState>,
}

impl EmbeddedDB {
    /// Opens an existing database or creates a new one.
    pub async fn open(path: &Path, options: EmbeddedDBOptions) -> Result<Self> {
        // TODO: Cache a file description to the data directory and use openat for
        // faster opens?

        let options = options.wrap_with_internal_keys();

        // TODO: Only do this if creating a new db. Also need to consider syncronization
        // concerns.
        async_std::fs::create_dir_all(path).await?;

        let dir = FilePaths::new(path.to_owned());

        let lock_file = {
            let mut opts = std::fs::OpenOptions::new();
            opts.write(true).create(true).read(true);

            let file = opts
                .open(dir.lock())
                .map_err(|_| err_msg("Failed to open the lockfile"))?;
            file.try_lock_exclusive()
                .map_err(|_| err_msg("Failed to lock database"))?;
            file
        };

        // TODO: Exists may ignore errors such as permission errors.
        let identity_path = dir.identity();
        let identity = if common::async_std::path::Path::new(&identity_path)
            .exists()
            .await
        {
            let data = async_std::fs::read_to_string(identity_path).await?;
            Some(common::hex::decode(&data.replace('-', ""))?)
        } else {
            None
        };

        let current_path = dir.current();
        let mut current = async_std::fs::read_to_string(current_path).await?;
        current = current.trim_end().to_string();

        let manifest_path = path.join(&current);
        let mut manifest_file = RecordReader::open(&manifest_path).await?;

        //	let mut manifest_data = vec![];
        //	manifest_file.read_to_end(&mut manifest_data).await?;

        /*

        pub comparator: Option<String>,
        pub log_number: Option<u64>,
        pub prev_log_number: Option<u64>,
        pub last_sequence: Option<u64>,
        pub new_files: Vec<NewFileEntry>,
        pub deleted_files: Vec<DeletedFileEntry>,
        pub next_file_number: Option<u64>,
        */

        // NOTE: LevelDB/RocksDB start at 2, where the first MANIFEST gets the
        // number 2 and the first log gets 3.

        let mut base_edit = VersionEdit::default();

        let mut levels = vec![];

        let mut highest_file_number_seen = 0;
        let mut deleted_files = HashSet::new();

        while let Some(edit) = VersionEdit::read(&mut manifest_file).await? {
            // TODO: Verify that all fields are merged.

            if edit.comparator.is_some() {
                if base_edit.comparator.is_some() {
                    return Err(err_msg("Not allowed to change the comparator of a DB"));
                }

                base_edit.comparator = edit.comparator;
            }

            if let Some(log_number) = edit.log_number {
                if log_number < 2 {
                    return Err(err_msg("Invalid log number"));
                }

                if let Some(base_log_number) = base_edit.log_number {
                    if base_log_number > log_number {
                        return Err(err_msg("Expected monotonic log numbers"));
                    }
                }

                base_edit.log_number = Some(log_number);
            }

            if let Some(prev_log_number) = edit.prev_log_number {
                if prev_log_number == 0 {
                    // This means that the previous log was deleted.

                    base_edit.prev_log_number = None;
                    continue;
                }

                base_edit.prev_log_number = Some(prev_log_number);
            }

            if let Some(last_sequence) = edit.last_sequence {
                if let Some(base_last_sequence) = base_edit.last_sequence {
                    if last_sequence < base_last_sequence {
                        return Err(err_msg(
                            "Expected only monotonically increasing sequence numbers",
                        ));
                    }
                }

                base_edit.last_sequence = Some(last_sequence);
            }

            if let Some(next_file_number) = edit.next_file_number {
                if let Some(base_next_file) = base_edit.next_file_number {
                    if next_file_number < base_next_file {
                        return Err(err_msg("Expected next_file_number to be monotonic"));
                    }
                }

                base_edit.next_file_number = Some(next_file_number);
            }

            for file in edit.new_files {
                if file.number <= highest_file_number_seen {}

                // if file.
            }
        }

        // if base_edit.next_file_number

        // Verify that next_file_number is at least highest_

        // let version_edit = VersionEdit::read(&mut manifest_file).await?;
        // println!("{:#?}", version_edit);

        let next_file_number = AtomicU64::new(version_edit.next_file_number.unwrap_or(2));

        let mut immutable_table = None;

        let prev_log_number = match version_edit.prev_log_number.unwrap_or(0) {
            0 => None,
            num @ _ => Some(num),
        };

        if let Some(num) = prev_log_number {
            let mut log = RecordReader::open(&dir.log(num)).await?;
            let mut table = MemTable::new(options.table_options.comparator.clone());
            table.apply_log(&mut log).await?;
            immutable_table = Some(table);
        }

        let log_number = match version_edit.log_number.unwrap_or(0) {
            0 => None,
            num @ _ => Some(num),
        }
        .unwrap();

        // Read both log files and put them into the memtable.

        let mut log = RecordReader::open(&dir.log(log_number)).await?;

        let mut mutable_table = MemTable::new(options.table_options.comparator.clone());
        mutable_table.apply_log(&mut log).await?;

        // TODO: Look up all files in the directory and delete any not-referenced
        // by the current log.

        Ok(Self {
            options,
            dir,
            lock_file,
            identity,
            next_file_number,
            state: RwLock::new(EmbeddedDBState {
                prev_log_number,
                log_number,
                log: log.into_writer(),
                mutable_table,
                immutable_table,
                level_tables: vec![],
            }),
        })
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

        // Check for all tables that would overlap with the desired key.
        // TODO: Will need a 'contains_prefix' incase a single user key was
        // shared across multiple tables.

        // Otherwise find all relevant SSTables, clone the references, unlock

        // Then using the cloned table references, look up the key until we
        // find it.

        Ok(None)
    }
}

pub struct EmbeddedDBState {
    log_number: u64,

    log: RecordWriter,

    /// If present, then this is the previous log number which corresponds to
    /// all values in the immutable_table. This file can be deleted once the
    /// immutable_table is flushed to disk.
    prev_log_number: Option<u64>,

    /// Primary table for reading/writing latest values.
    mutable_table: MemTable,

    /// Immutable table currently being written to disk.
    immutable_table: Option<MemTable>,

    version_set: VersionSet,
}

// TODO: See here for all RocksDB options:
// https://github.com/facebook/rocksdb/blob/6ec6a4a9a49e506eff76aebd104d30be6a2d36cc/include/rocksdb/options.h#L348
#[derive(Defaultable)]
pub struct EmbeddedDBOptions {
    /// While opening, if no database exists yet, create a new empty one.
    pub create_if_missing: bool,

    pub error_if_exists: bool,

    /// Max amount of data to store in memory before the data is flushed into an
    /// SSTable.
    ///
    /// Default 64MB in RocksDB, 4MB in LevelDB
    #[default(64*1024*1024)]
    pub write_buffer_size: usize,

    /// After we have accumulated this many files in level 0, we will trigger
    /// compaction into level 1.
    #[default(4)]
    pub level0_file_num_compaction_trigger: usize,

    /// Base for computing the target file size for individual files at each
    /// level. This is the value for level 1. For other levels, the file size
    /// is computed as 'base*(multiplier^(level - 1))'. After a file reaches
    /// that size, a new file will be created.
    ///
    /// This option has the same name in RocksDB with default value 64MB,
    /// but is called max_file_size in LevelDB with default value 2MB.
    #[default(64*1024*1024)]
    pub target_file_size_base: usize,

    /// Defaults to 1.
    #[default(1)]
    pub target_file_size_multiplier: usize,

    /// Base for computing the maximum size of each level. This will be the size
    /// of level 1, and every additional level will have size:
    /// 'base*(multiplier^(level - 1))'
    #[default(256*1024*1024)]
    pub max_bytes_for_level_base: usize, // = 256 * 1048576;

    /// Default 10 for RocksDB.
    #[default(10)]
    pub max_bytes_for_level_multiplier: usize,

    /// Options to use for building tables on disk.
    pub table_options: SSTableBuilderOptions,
    /*	/// If true, open the database in write mode, otherwise, the opened database
     *	/// will be read-only.
     *	pub writeable: bool, */

    // TODO: Limit max number of open files.

    /* max_log_file_size */
    #[default(1024*1024*1024)]
    pub max_manifest_file_size: usize,

    #[default(4*1024*1024)]
    pub manifest_preallocation_size: usize,

    #[default(2)]
    pub max_background_jobs: usize,
}

impl EmbeddedDBOptions {
    pub fn wrap_with_internal_keys(mut self) -> Self {
        self.table_options.comparator = InternalKeyComparator::wrap(self.table_options.comparator);
        self.table_options.filter_policy = self
            .table_options
            .filter_policy
            .map(|policy| InternalKeyFilterPolicy::wrap(policy));
        self
    }
}

/// Accessor for all file paths contained within a database directory.
pub struct FilePaths {
    root_dir: PathBuf,
}

impl FilePaths {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    /// Empty file used to guarantee that exactly one process is accessing the
    /// DB data directory at a single time.
    ///
    /// The lock is engaged via sycalls, namely fcntl(.., F_SETLK)
    pub fn lock(&self) -> PathBuf {
        self.root_dir.join("LOCK")
    }

    /// File containing the database UUID.
    ///
    /// Only present in RocksDB compatible databases. Note that RocksDB by
    /// default doesn't write the uuid to the manifest and only only writes it
    /// to this file.
    pub fn identity(&self) -> PathBuf {
        self.root_dir.join("IDENTITY")
    }

    /// File that contains the filename of the currently active manifest.
    pub fn current(&self) -> PathBuf {
        self.root_dir.join("CURRENT")
    }

    pub fn log(&self, num: u64) -> PathBuf {
        self.root_dir.join(format!("{:06}.log", num))
    }

    pub fn manifest(&self, num: u64) -> PathBuf {
        self.root_dir.join(format!("MANIFEST-{:06}", num))
    }

    // TODO: Eventually should we support cleaning up unknown files in the data
    // directory?
}

struct EmbeddedDBFile {
    table: SSTable,
    entry: NewFileEntry,
}

struct EmbeddedDBIterator {
    // TODO: Need a Mem-Table iterator (or at least a small reference)
    /// At each level, we need to know the file number for the purpose of
    /// seeking the next one. (Although we may need to
    levels: Vec<(usize, SSTableIterator)>,
}
