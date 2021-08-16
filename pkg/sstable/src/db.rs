use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;

use common::async_std;
use common::async_std::fs::{File, OpenOptions};
use common::async_std::sync::{Arc, RwLock};
use common::errors::*;
use fs2::FileExt;

use crate::internal_key::*;
use crate::manifest::*;
use crate::memtable::*;
use crate::record_log::*;
use crate::table::{SSTable, SSTableIterator};
use crate::table_builder::{SSTableBuilder, SSTableBuilderOptions};
use crate::write_batch::Write::Value;
use crate::write_batch::*;

// TODO: See https://github.com/google/leveldb/blob/c784d63b931d07895833fb80185b10d44ad63cce/db/filename.cc#L78 for all owned files

/*
    Flushing a table to disk:
    - Make the mutable_table immutable (and simulataneously swap to a new log file).
    - Create a new SSTable on disk
    - Write a new version of the MANIFEST pointing to an empty log file
*/
// TODO: Before deleting all un-used files, be sure to use absolute paths.

// TODO: Should implement read/write options like: https://github.com/google/leveldb/blob/9bd23c767601a2420478eec158927882b879bada/include/leveldb/options.h#L146

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
        let options = options.wrap_internal_keys();

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
        let identity_path = path.join("IDENTITY");
        let identity = if common::async_std::path::Path::new(&identity_path)
            .exists()
            .await
        {
            let data = async_std::fs::read_to_string(identity_path).await?;
            Some(common::hex::decode(&data.replace('-', ""))?)
        } else {
            None
        };

        let current_path = path.join("CURRENT");
        let mut current = async_std::fs::read_to_string(current_path).await?;
        current = current.trim_end().to_string();

        let manifest_path = path.join(&current);
        let mut manifest_file = RecordReader::open(&manifest_path).await?;

        //	let mut manifest_data = vec![];
        //	manifest_file.read_to_end(&mut manifest_data).await?;

        let version_edit = VersionEdit::read(&mut manifest_file).await?;
        println!("{:#?}", version_edit);

        // NOTE: LevelDB/RocksDB start at 2, where the first MANIFEST gets the
        // number 2 and the first log gets 3.
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

    pub async fn get(&self, user_key: &[u8]) -> Result<Option<Vec<u8>>> {
        let seek_ikey = InternalKey::before(user_key).serialized();
        let snapshot_sequence = 0xffffff; // TODO:

        let state = self.state.read().await;

        let get_from_memtable = |memtable: &MemTable| -> Option<Option<Vec<u8>>> {
            // The first value should be the one with the highest value.
            let mut iter = memtable.range_from(&seek_ikey);
            for (key, value) in iter {
                let ikey = InternalKey::parse(key).unwrap();

                // TODO: Use user comparator.
                if ikey.user_key == user_key {
                    if ikey.typ == ValueType::Deletion {
                        return Some(None);
                    } else if ikey.sequence <= snapshot_sequence {
                        return Some(Some(value.to_vec()));
                    }
                } else {
                    break;
                }
            }

            None
        };

        // Try both memtables.
        if let Some(result) = get_from_memtable(&state.mutable_table) {
            return Ok(result);
        }
        if let Some(table) = &state.immutable_table {
            if let Some(result) = get_from_memtable(table) {
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
    ///
    prev_log_number: Option<u64>,

    log_number: u64,

    log: RecordWriter,

    /// Primary table for reading/writing latest values.
    mutable_table: MemTable,

    /// Immutable table currently being written to disk.
    immutable_table: Option<MemTable>,

    level_tables: Vec<Vec<EmbeddedDBFile>>,
}

// TODO: See here for all RocksDB options:
// https://github.com/facebook/rocksdb/blob/6ec6a4a9a49e506eff76aebd104d30be6a2d36cc/include/rocksdb/options.h#L348
#[derive(Defaultable)]
pub struct EmbeddedDBOptions {
    /// While opening, if no database exists yet, create a new empty one.
    pub create_if_missing: bool,

    pub error_if_exists: bool,

    /// Default 64MB in RocksDB, 4MB in LevelDB
    #[default(64*1024*1024)]
    pub write_buffer_size: usize,

    #[default(4)]
    pub level0_file_num_compaction_trigger: usize,

    /// Base for computing the target file size for individual files at each
    /// level. This is the value for level 1. For other levels, the file size
    /// is computed as 'base^(level - 1)'. After a file reaches that size, a
    /// new file will be created.
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
    pub max_bytes_for_level_base: usize, // = 256 * 1048576;

    /// Default 10 for RocksDB.
    #[default(10)]
    pub max_bytes_for_level_multiplier: usize,

    /// Options to use for building tables on disk.
    pub table_options: SSTableBuilderOptions,
    /*	/// If true, open the database in write mode, otherwise, the opened database
     *	/// will be read-only.
     *	pub writeable: bool, */

    /* max_log_file_size */

    /* size_t manifest_preallocation_size = 4 * 1024 * 1024; */

    /*  */
}

impl EmbeddedDBOptions {
    pub fn wrap_internal_keys(mut self) -> Self {
        self.table_options.comparator = InternalKeyComparator::wrap(self.table_options.comparator);
        self.table_options.filter_policy = self
            .table_options
            .filter_policy
            .map(|policy| InternalKeyFilterPolicy::wrap(policy));
        self
    }
}

pub struct FilePaths {
    root_dir: PathBuf,
}

impl FilePaths {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    pub fn lock(&self) -> PathBuf {
        self.root_dir.join("LOCK")
    }

    pub fn identity(&self) -> PathBuf {
        self.root_dir.join("IDENTITY")
    }

    pub fn log(&self, num: u64) -> PathBuf {
        self.root_dir.join(format!("{:06}.log", num))
    }
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
