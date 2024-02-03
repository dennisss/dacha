use std::cmp::Ordering;
use std::collections::HashSet;
use std::f32::MAX_10_EXP;
use std::sync::Arc;

use common::algorithms::lower_bound_by;
use common::errors::*;
use executor::channel;
use executor::sync::AsyncMutex;

use crate::db::paths::FilePaths;
use crate::db::version_edit::NewFileEntry;
use crate::db::version_edit::{DeletedFileEntry, VersionEdit};
use crate::record_log::{RecordReader, RecordWriter};
use crate::table::comparator::KeyComparator;
use crate::table::table::{SSTable, SSTableOpenOptions};
use crate::EmbeddedDBOptions;

/// TODO: Implement this.
const MAX_NUM_LEVELS: usize = 7;

/// The highest level at which we will store a new SSTable resulting from a
/// memtable flush.
const MAX_MEMTABLE_LEVEL: usize = 2;

pub type FileReleasedCallback = Arc<dyn Fn(u64) + Send + Sync + 'static>;

/// A set of versions.
///
/// More specifically we maintain:
/// - The value of the current version is
pub struct VersionSet {
    options: Arc<EmbeddedDBOptions>,

    latest_version: Arc<Version>,

    next_file_number: u64,

    /// Number of the current log file.
    /// Once a database is succesfully created, this will always be non-None.
    ///
    /// The main exception is when EmbeddedDBOptions::disable_wal is enabled,
    /// then this and prev_log_number will always be None.
    log_number: Option<u64>,

    /// If present, then this is the previous log number which corresponds to
    /// all values in the immutable_table. This file can be deleted once the
    /// immutable_table is flushed to disk.
    prev_log_number: Option<u64>,

    /// Last sequence flushed to tables (excluding recent entries in the WAL).
    last_sequence: u64,

    release_callback: FileReleasedCallback,
}

impl VersionSet {
    /// Creates a new completely empty set containing new files.
    pub fn new(release_callback: FileReleasedCallback, options: Arc<EmbeddedDBOptions>) -> Self {
        Self {
            options,
            latest_version: Arc::new(Version::new()),
            // NOTE: LevelDB/RocksDB start at 2, where the first MANIFEST gets the
            // number 2 and the first log gets 3.
            next_file_number: 2,
            log_number: None,
            prev_log_number: None,
            last_sequence: 0,
            release_callback,
        }
    }

    pub fn latest_version(&self) -> &Arc<Version> {
        &self.latest_version
    }

    pub fn next_file_number(&self) -> u64 {
        self.next_file_number
    }

    pub fn log_number(&self) -> Option<u64> {
        self.log_number.clone()
    }

    pub fn prev_log_number(&self) -> Option<u64> {
        self.prev_log_number.clone()
    }

    pub fn last_sequence(&self) -> u64 {
        self.last_sequence
    }

    /// Writes a complete snapshot of this object to the given log file.
    ///
    /// - 'writer' should refer to an empty log file.
    /// - This can later be restored using 'recover_existing'
    pub async fn write_to_new(&self, exclude_logs: bool, writer: &mut RecordWriter) -> Result<()> {
        let edit = self.to_version_edit(exclude_logs);

        let mut out = vec![];
        edit.serialize(&mut out)?;
        writer.append(&out).await?;

        Ok(())
    }

    pub fn to_version_edit(&self, exclude_logs: bool) -> VersionEdit {
        let mut edit = VersionEdit::default();
        edit.next_file_number = Some(self.next_file_number);
        if !exclude_logs {
            edit.log_number = self.log_number.clone();
            edit.prev_log_number = self.prev_log_number.clone();
        }
        edit.last_sequence = Some(self.last_sequence);
        edit.comparator = Some(self.options.table_options.comparator.name().to_string());

        for level in &self.latest_version.levels {
            for table in &level.tables {
                edit.new_files.push(table.entry.clone());
            }
        }

        edit
    }

    pub async fn recover_existing(
        reader: &mut RecordReader,
        release_callback: FileReleasedCallback,
        options: Arc<EmbeddedDBOptions>,
    ) -> Result<Self> {
        let mut version = Version::new();

        let mut base_edit = VersionEdit::default();

        let mut highest_file_number_seen = 0;

        while let Some(mut edit) = VersionEdit::read(reader).await? {
            // println!("EDIT: {:#?}", edit);

            // TODO: Verify that all fields are merged.

            if edit.comparator.is_some() {
                if base_edit.comparator.is_some() {
                    return Err(err_msg("Not allowed to change the comparator of a DB"));
                }

                // TODO: Change the comparator right here.
                if options.table_options.comparator.name() != edit.comparator.as_ref().unwrap() {
                    return Err(err_msg("Mistmatch in comparator"));
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

                if log_number < highest_file_number_seen {
                    return Err(err_msg(
                        "New log number smaller than largest file number seen",
                    ));
                }

                // This is updated later down.
                // highest_file_number_seen = log_number;

                base_edit.log_number = Some(log_number);
            }

            if let Some(prev_log_number) = edit.prev_log_number {
                if prev_log_number == 0 {
                    // This means that the previous log was deleted.
                    base_edit.prev_log_number = None;
                } else {
                    base_edit.prev_log_number = Some(prev_log_number);
                }
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

            for file in edit.deleted_files {
                if !version.remove(&file) {
                    return Err(err_msg("Failed to delete non-existent file"));
                }
            }

            edit.new_files.sort_by_key(|f| f.number);

            for file in edit.new_files {
                if file.number <= highest_file_number_seen {
                    return Err(err_msg("Already saw a file that a >= file number"));
                }

                highest_file_number_seen = file.number;

                if !base_edit.comparator.is_some() {
                    return Err(err_msg("Creating new file before comparator defined"));
                }

                version.insert(file, None, release_callback.clone(), &options);
            }

            // RocksDB may re-use the same number for logs and tables so for compatibility
            // we only require increasing log numbers across different edits.
            if let Some(log_num) = base_edit.log_number {
                highest_file_number_seen = highest_file_number_seen.max(log_num);
            }
        }

        // TODO: Check if leveldb can ever be in a state where this is not set but the
        // CURRENT file was written.
        //
        // TODO: Eventually we will also need to account for all extra WAL files in the
        // directory that rocksdb hasn't marked in the manifest.
        let next_file_number = base_edit
            .next_file_number
            .ok_or_else(|| err_msg("Manifest missing a next_file_number"))?;

        if next_file_number <= highest_file_number_seen {
            return Err(err_msg("next_file_number too small"));
        }

        Ok(Self {
            options,
            latest_version: Arc::new(version),
            last_sequence: base_edit.last_sequence.unwrap_or(0),
            next_file_number,
            log_number: base_edit.log_number,
            prev_log_number: base_edit.prev_log_number,
            release_callback,
        })
    }

    pub fn apply_new_edit(&mut self, version_edit: VersionEdit, new_tables: Vec<SSTable>) {
        if let Some(last_sequence) = version_edit.last_sequence {
            assert!(last_sequence >= self.last_sequence);
            self.last_sequence = last_sequence;
        }

        if let Some(next_file_number) = version_edit.next_file_number {
            assert!(next_file_number >= self.next_file_number);
            self.next_file_number = next_file_number;
        }

        if let Some(log_number) = version_edit.log_number {
            self.log_number = Some(log_number);
        }

        if let Some(prev_log_number) = version_edit.prev_log_number {
            if prev_log_number == 0 {
                self.prev_log_number = None;
            } else {
                self.prev_log_number = Some(prev_log_number);
            }
        }

        if !version_edit.new_files.is_empty() || !version_edit.deleted_files.is_empty() {
            let version = Arc::make_mut(&mut self.latest_version);

            for entry in &version_edit.deleted_files {
                assert!(version.remove(&entry));
            }

            for (entry, table) in version_edit
                .new_files
                .into_iter()
                .zip(new_tables.into_iter())
            {
                version.insert(
                    entry,
                    Some(Arc::new(table)),
                    self.release_callback.clone(),
                    &self.options,
                );
            }
        }
    }

    /// Open all not currently opened tables in this version.
    pub async fn open_all(&self, dir: &FilePaths) -> Result<()> {
        let options = SSTableOpenOptions {
            comparator: self.options.table_options.comparator.clone(),
            block_cache: self.options.block_cache.clone(),
            filter_registry: self.options.filter_registry.clone(),
        };

        // TODO: Parallelize me.
        for level in &self.latest_version.levels {
            for entry in &level.tables {
                let mut table = entry.table.lock().await?.enter();
                if table.is_none() {
                    *table = Some(Arc::new(
                        SSTable::open(dir.table(entry.entry.number), options.clone()).await?,
                    ));
                }

                table.exit();
            }
        }

        Ok(())
    }

    // TODO: Deduplicate with the other code.
    pub fn target_file_size(&self, mut level: u32) -> u64 {
        if level == 0 {
            level = 1;
        }

        self.options.target_file_size_base * self.options.target_file_size_multiplier.pow(level - 1)
    }

    pub fn pick_memtable_level(&self, key_range: KeyRangeRef) -> SelectedMemtableLevel {
        let mut highest_level = 0;
        let mut found_overlap = false;

        for level in 0..self.latest_version.levels.len() {
            for table in &self.latest_version.levels[level].tables {
                if key_range.overlaps_with(
                    table.key_range(),
                    self.options.table_options.comparator.as_ref(),
                ) {
                    found_overlap = true;
                    break;
                }
            }

            if found_overlap {
                break;
            } else {
                highest_level = level;
            }
        }

        // In this case, self.latest_version.levels.len() < MAX_MEMTABLE_LEVEL
        if !found_overlap && highest_level < MAX_MEMTABLE_LEVEL {
            highest_level = MAX_MEMTABLE_LEVEL;
        }

        SelectedMemtableLevel {
            level: std::cmp::min(MAX_MEMTABLE_LEVEL, highest_level),
            found_overlap,
        }
    }

    pub fn select_tables_to_compaction(&self) -> Option<CompactionSpec> {
        let level_num;
        let tables;

        // Compacting level 0 is just based on the quantity of files and we always
        // compact all tables in the level.
        if self.latest_version.levels.len() > 1
            && self.latest_version.levels[0].tables.len()
                >= self.options.level0_file_num_compaction_trigger
        {
            level_num = 0;
            tables = &self.latest_version.levels[0].tables[..];
        } else {
            // Set level_num to the first level which is over it's limit.
            {
                let mut maybe_level_num = None;
                for level_num in 1..self.latest_version.levels.len() {
                    let level = &self.latest_version.levels[level_num];
                    if level.total_size > level.max_size {
                        maybe_level_num = Some(level_num);
                        break;
                    }
                }

                if let Some(num) = maybe_level_num {
                    level_num = num;
                } else {
                    return None;
                }
            }

            let level = &self.latest_version.levels[level_num];

            // Find a random contiguous range of tables in this level which we can remove in
            // order to get us below the max_size.
            // TODO: Need to also expand to any adjacent takes that contain a boundary user
            // key.
            let mut i = (crypto::random::clocked_rng().next_u32() as usize) % level.tables.len();
            let mut j = i;
            let mut new_total_size = level.total_size;
            while new_total_size > level.max_size {
                if j < level.tables.len() {
                    new_total_size -= level.tables[j].entry.file_size;
                    j += 1;
                } else if i > 0 {
                    i -= 1;
                    new_total_size -= level.tables[j].entry.file_size;
                } else {
                    break;
                }
            }

            tables = &level.tables[i..j];
        }

        assert!(tables.len() > 0);

        let comparator = self.options.table_options.comparator.as_ref();

        let mut key_range = tables[0].key_range();
        for table in &tables[1..] {
            key_range = key_range.union(table.key_range(), comparator);
        }

        let next_level = level_num + 1;

        // Find all tables in the next level that
        let mut next_level_tables: &[Arc<LevelTableEntry>] = &[];
        if next_level < self.latest_version.levels.len() {
            next_level_tables = &self.latest_version.levels[next_level].tables;

            // TODO: This should be optimized by finding the start index first with binary
            // search.
            while next_level_tables.len() > 0 {
                if !next_level_tables[0]
                    .key_range()
                    .overlaps_with(key_range, comparator)
                {
                    next_level_tables = &next_level_tables[1..];
                } else if !next_level_tables[next_level_tables.len() - 1]
                    .key_range()
                    .overlaps_with(key_range, comparator)
                {
                    next_level_tables = &next_level_tables[..(next_level_tables.len() - 1)];
                } else {
                    break;
                }
            }
        }

        let mut found_overlap = false;
        for i in (next_level + 1)..self.latest_version.levels.len() {
            for table in &self.latest_version.levels[i].tables {
                if table.key_range().overlaps_with(key_range, comparator) {
                    found_overlap = true;
                    break;
                }
            }

            if found_overlap {
                break;
            }
        }

        Some(CompactionSpec {
            level: level_num,
            tables,
            next_level,
            next_level_tables,
            found_overlap,
        })
    }
}

pub struct SelectedMemtableLevel {
    pub level: usize,
    pub found_overlap: bool,
}

pub struct CompactionSpec<'a> {
    pub level: usize,
    pub tables: &'a [Arc<LevelTableEntry>],

    pub next_level: usize,
    pub next_level_tables: &'a [Arc<LevelTableEntry>],

    pub found_overlap: bool,
}

/// A single immutable point in time view of all the tables written to disk.
#[derive(Clone)]
pub struct Version {
    /// All tables stored on disk.
    /// level_tables[i] corresponds to all tables in the i'th level.
    ///
    /// All level vectors other than level_tables[0] are in sorted order by
    /// smallest key and are non-overlapping in key ranges.
    pub levels: Vec<Level>,
}

#[derive(Clone)]
pub struct Level {
    /// Total size in bytes of all tables in this level.
    pub total_size: u64,

    /// Maximum number of bytes allowed to be in this level. If we exceed this,
    /// we will soon try to compact some tables into the next level.
    pub max_size: u64,

    pub target_file_size: u64,

    /// List is all tables in this level.
    ///
    /// If this is level 0, then this will be ordered from oldest to newest
    /// table with a newer table possibly containing overlappign keys with older
    /// tables.
    ///
    /// Else for other levels, this will be list of tables with non-overlapping
    /// key ranges sorted by the first key in each table.
    pub tables: Vec<Arc<LevelTableEntry>>,
}

pub struct LevelTableEntry {
    /// Opened table reference. May be None if we have too many files open and
    /// had to close a table.
    table: AsyncMutex<Option<Arc<SSTable>>>,

    pub entry: NewFileEntry,

    release_callback: Option<FileReleasedCallback>,
}

impl Drop for LevelTableEntry {
    fn drop(&mut self) {
        if let Some(release_callback) = self.release_callback.take() {
            let file_num = self.entry.number;
            (*release_callback)(file_num);
        }
    }
}

impl LevelTableEntry {
    pub async fn table(&self) -> Arc<SSTable> {
        self.table
            .lock()
            .await
            .unwrap()
            .read_exclusive()
            .as_ref()
            .unwrap()
            .clone()
    }

    pub fn key_range(&self) -> KeyRangeRef {
        KeyRangeRef {
            smallest: &self.entry.smallest_key,
            largest: &self.entry.largest_key,
        }
    }
}

impl Version {
    pub fn new() -> Self {
        Self { levels: vec![] }
    }

    /// NOTE: This does not validate the new entry is not already present or
    /// doesn't overlap with other tables.
    pub fn insert(
        &mut self,
        entry: NewFileEntry,
        table: Option<Arc<SSTable>>,
        release_callback: FileReleasedCallback,
        db_options: &EmbeddedDBOptions,
    ) {
        while self.levels.len() <= entry.level as usize {
            let number = self.levels.len() as u32;
            if number == 0 {
                self.levels.push(Level {
                    total_size: 0,
                    max_size: 0,
                    target_file_size: 0,
                    tables: vec![],
                });
                continue;
            }

            let max_size = db_options.max_bytes_for_level_base
                * db_options.max_bytes_for_level_multiplier.pow(number - 1);

            let target_file_size = db_options.target_file_size_base
                * db_options.target_file_size_multiplier.pow(number - 1);

            self.levels.push(Level {
                total_size: 0,
                max_size,
                target_file_size,
                tables: vec![],
            });
        }

        if entry.level == 0 {
            self.levels[0].tables.push(Arc::new(LevelTableEntry {
                table: AsyncMutex::new(table),
                entry,
                release_callback: Some(release_callback),
            }));
            return;
        }

        let level = &mut self.levels[entry.level as usize];

        let idx = lower_bound_by(&level.tables[..], &entry, |e1, e2| {
            e1.entry
                .compare(e2, db_options.table_options.comparator.as_ref())
                .is_ge()
        })
        .unwrap_or(level.tables.len());

        level.tables.insert(
            idx,
            Arc::new(LevelTableEntry {
                table: AsyncMutex::new(table),
                entry,
                release_callback: Some(release_callback),
            }),
        );
    }

    pub fn remove(&mut self, entry: &DeletedFileEntry) -> bool {
        let level = &mut self.levels[entry.level as usize];
        let idx = level
            .tables
            .iter()
            .position(|e| e.entry.number == entry.number);

        if let Some(idx) = idx {
            level.tables.remove(idx);
            true
        } else {
            false
        }
    }
}

#[derive(Clone, Copy)]
pub struct KeyRangeRef<'a> {
    pub smallest: &'a [u8],
    pub largest: &'a [u8],
}

impl<'a> KeyRangeRef<'a> {
    pub fn overlaps_with(&self, other: KeyRangeRef, comparator: &dyn KeyComparator) -> bool {
        match comparator.compare(self.smallest, other.smallest) {
            Ordering::Equal => true,
            Ordering::Less => comparator.compare(self.largest, other.smallest).is_ge(),
            Ordering::Greater => comparator.compare(self.smallest, other.largest).is_le(),
        }
    }

    /// Creates a new key range which contains all keys in two ranges.
    pub fn union(&self, other: KeyRangeRef<'a>, comparator: &dyn KeyComparator) -> KeyRangeRef<'a> {
        let smallest = {
            if comparator.compare(self.smallest, other.smallest).is_le() {
                self.smallest
            } else {
                other.smallest
            }
        };

        let largest = {
            if comparator.compare(self.largest, other.largest).is_ge() {
                self.largest
            } else {
                other.largest
            }
        };

        KeyRangeRef { smallest, largest }
    }
}
