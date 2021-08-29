use std::collections::HashSet;
use std::sync::Arc;

use common::algorithms::lower_bound_by;
use common::async_std::sync::Mutex;
use common::errors::*;

use crate::db::paths::FilePaths;
use crate::db::version_edit::NewFileEntry;
use crate::db::version_edit::{DeletedFileEntry, VersionEdit};
use crate::record_log::{RecordReader, RecordWriter};
use crate::table::table::{SSTable, SSTableOpenOptions};
use crate::EmbeddedDBOptions;

const MAX_NUM_LEVELS: usize = 7;

pub struct VersionSet {
    pub options: Arc<EmbeddedDBOptions>,

    pub latest_version: Arc<Version>,

    pub next_file_number: u64,

    /// Number of the current log file.
    /// Once a database is succesfully created, this will always be non-None.
    pub log_number: Option<u64>,

    /// If present, then this is the previous log number which corresponds to
    /// all values in the immutable_table. This file can be deleted once the
    /// immutable_table is flushed to disk.
    pub prev_log_number: Option<u64>,

    pub last_sequence: u64,
}

impl VersionSet {
    pub fn new(options: Arc<EmbeddedDBOptions>) -> Self {
        Self {
            options,
            latest_version: Arc::new(Version::new()),
            // NOTE: LevelDB/RocksDB start at 2, where the first MANIFEST gets the
            // number 2 and the first log gets 3.
            next_file_number: 2,
            log_number: None,
            prev_log_number: None,
            last_sequence: 0,
        }
    }

    pub async fn write_to_new(&self, writer: &mut RecordWriter) -> Result<()> {
        let mut edit = VersionEdit::default();
        edit.next_file_number = Some(self.next_file_number);
        edit.log_number = self.log_number.clone();
        edit.prev_log_number = Some(self.prev_log_number.unwrap_or(0));
        edit.last_sequence = Some(self.last_sequence);
        edit.comparator = Some(self.options.table_options.comparator.name().to_string());

        for level in &self.latest_version.levels {
            for table in &level.tables {
                edit.new_files.push(table.entry.clone());
            }
        }

        let mut out = vec![];
        edit.serialize(&mut out)?;
        writer.append(&out).await?;

        Ok(())
    }

    pub async fn recover_existing(
        reader: &mut RecordReader,
        options: Arc<EmbeddedDBOptions>,
    ) -> Result<Self> {
        let mut version = Version::new();

        let mut base_edit = VersionEdit::default();

        let mut highest_file_number_seen = 0;

        while let Some(edit) = VersionEdit::read(reader).await? {
            println!("EDIT: {:#?}", edit);

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

                highest_file_number_seen = log_number;

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

            for file in edit.new_files {
                if file.number <= highest_file_number_seen {
                    return Err(err_msg("Already saw a file that a >= file number"));
                }

                highest_file_number_seen = file.number;

                if !base_edit.comparator.is_some() {
                    return Err(err_msg("Creating new file before comparator defined"));
                }

                version.insert(file, &options);
            }

            for file in edit.deleted_files {
                if !version.remove(&file) {
                    return Err(err_msg("Failed to delete non-existent file"));
                }
            }
        }

        // TODO: Check if leveldb can ever be in a state where this is not set but the
        // CURRENT file was written.
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
        })
    }
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
    /// Total size in bytes of all tables in this level
    pub total_size: u64,

    /// Maximum number of bytes allowed to be in this level until we
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
    pub table: Mutex<Option<Arc<SSTable>>>,

    pub entry: NewFileEntry,
}

impl Version {
    pub fn new() -> Self {
        Self { levels: vec![] }
    }

    /// NOTE: This does not validate the new entry is not already present or
    /// doesn't overlap with other tables.
    pub fn insert(&mut self, entry: NewFileEntry, db_options: &EmbeddedDBOptions) {
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
                table: Mutex::new(None),
                entry,
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
                table: Mutex::new(None),
                entry,
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

    /// Open all not currently opened tables in this version.
    pub async fn open_all(&self, dir: &FilePaths) -> Result<()> {
        /*
        let mut options = sstable::table::table::SSTableOpenOptions {
            comparator: Arc::new(BytewiseComparator::new()),
        };
        */

        // TODO: Parallelize me.
        for level in &self.levels {
            for entry in &level.tables {
                let mut table = entry.table.lock().await;
                if table.is_none() {
                    // table = Some(Arc::new(SSTable::open(path,
                    // SSTableOpenOptions::default())))
                }
            }
        }

        Ok(())
    }
}
