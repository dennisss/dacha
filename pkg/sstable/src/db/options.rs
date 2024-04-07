use std::sync::Arc;

use crate::db::internal_key::*;
use crate::table::filter_policy::FilterPolicyRegistry;
use crate::table::table::DataBlockCache;
use crate::table::table_builder::SSTableBuilderOptions;

// TODO: Should support prefix_extractor:
// https://github.com/facebook/rocksdb/blob/9502856edd77260bf8a12a66f2a232078ddb2d60/include/rocksdb/options.h#L275
// - Will make Bloom filtering more efficient.

/// Options to use for opening a new or existing EmbeddedDB instance.
///
/// This is meant to be mostly compatible with RocksDB:
/// https://github.com/facebook/rocksdb/blob/6ec6a4a9a49e506eff76aebd104d30be6a2d36cc/include/rocksdb/options.h#L348
#[derive(Defaultable)]
pub struct EmbeddedDBOptions {
    /// While opening, if no database exists yet, create a new empty one.
    ///
    /// NOTE: The existence of a database is defined by whether or not the
    /// CURRENT file is present in the directory. If that file isn't present,
    /// then we may overwrite any existing partially written data in the
    /// directory that was created during a previous attempt to create the
    /// database.
    pub create_if_missing: bool,

    /// Returns an error if the database already exists.
    pub error_if_exists: bool,

    /// TODO: Implement this. Basically block insertions and disable the
    /// background thread.
    // Also we can check using the memtable on reads.
    pub read_only: bool,

    /// Max amount of data to store in memory before the data is flushed into an
    /// SSTable.
    ///
    /// Default 64MB in RocksDB, 4MB in LevelDB
    #[default(64*1024*1024)]
    pub write_buffer_size: usize,

    /// The highest level at which we will store a new SSTable resulting from a
    /// memtable flush.
    #[default(2)]
    pub max_memtable_level: usize,

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
    pub target_file_size_base: u64,

    /// Defaults to 1.
    #[default(1)]
    pub target_file_size_multiplier: u64,

    /// Base for computing the maximum size of each level. This will be the size
    /// of level 1, and every additional level will have size:
    /// 'base*(multiplier^(level - 1))'
    #[default(256*1024*1024)]
    pub max_bytes_for_level_base: u64,

    /// Default 10 for RocksDB.
    #[default(10)]
    pub max_bytes_for_level_multiplier: u64,

    /// Options to use for building tables on disk.
    pub table_options: SSTableBuilderOptions,

    /// Filter registry to use when looking up the filters used by existing
    /// (already written to disk) tables.
    pub filter_registry: Arc<FilterPolicyRegistry>,

    /// Maximum size of the current manifest log file. After this is exceeded,
    /// we will switch to using a new log file.
    #[default(1024*1024*1024)]
    pub max_manifest_file_size: u64,

    /// TODO: Implement.
    #[default(4*1024*1024)]
    pub manifest_preallocation_size: u64,

    /// TODO: Implement.
    #[default(2)]
    pub max_background_jobs: usize,

    /// 8MB is the default in LevelDB.
    #[default(DataBlockCache::new(8 * 1024 * 1024))]
    pub block_cache: DataBlockCache,

    /// FOR TESTING ONLY
    ///
    /// If true, we will not perform automatic compaction. Instead compaction
    /// will only occur on manual calls by the user to wait_for_compactions().
    pub manual_compactions_only: bool,

    /// If true, we will not maintain a write ahead log. This means that
    /// database writes will not block on disk flushing. Unpersisted entries
    /// will only be flushed to disk if manual flushing is performed and if
    /// enough data has been accumulated to exceed the 'write_buffer_size'.
    ///
    /// Note that an existing database can only be re-opened with the same
    /// disable_wal value used to create it.
    pub disable_wal: bool,

    /// If given a non-zero value N, we will not garbage collect any mutations
    /// with a sequence number > N. This includes both Put and Delete
    /// mutations.
    ///
    /// NOTE: This option can NOT be turned off or on. It must stay the same
    /// since database initialization.
    ///
    /// - If a key has a real non-deleted value, the latest value entry will
    ///   never be deleted.
    /// - Stale puts/delete entries (those do not have the latest sequence for a
    ///   given user key) will only be compacted if the next newest put/delete
    ///   entry is also below the compaction_waterline.
    ///   - This is to prevent an old entry from immediately being compacted as
    ///     soon as a newer entry is added.
    ///
    /// WARNING: It is the caller's responsible to verify that only
    /// non-compacted sequence ranges are used when querying a database with
    /// this enabled.
    pub enable_compaction_waterline: bool,

    /// What fraction (from 0 to 1) of entries in a table need to be stale and
    /// compactable for us to consider re-generating the table in-place.
    ///
    /// This is mainly relevant when using a initial_compaction_waterline > 0
    /// since otherwise stale entries are immediately dropped when writing to
    /// disk.
    #[default(0.5)]
    pub stale_compaction_threshold: f32,
}

impl EmbeddedDBOptions {
    pub fn wrap_with_internal_keys(mut self) -> Self {
        self.table_options.comparator = InternalKeyComparator::wrap(self.table_options.comparator);
        self.table_options.filter_policy = self
            .table_options
            .filter_policy
            .map(|policy| InternalKeyFilterPolicy::wrap(policy));
        self.filter_registry = Arc::new(
            self.filter_registry
                .wrap(|policy| InternalKeyFilterPolicy::wrap(policy)),
        );
        self
    }
}
