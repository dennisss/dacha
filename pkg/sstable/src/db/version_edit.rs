use std::cmp::Ordering;
use std::collections::HashSet;

use common::errors::*;
use protobuf::wire::{parse_varint, serialize_varint};

use crate::encoding::*;
use crate::record_log::{RecordReader, RecordWriter};
use crate::table::comparator::KeyComparator;

// See https://github.com/facebook/rocksdb/blob/5f025ea8325a2ff5239ea28365073bf0b723514d/db/version_edit.cc#L29 for the complete list of tags
// Also https://github.com/google/leveldb/blob/master/db/version_edit.cc

// NOTE: A sequence number is a u64

enum_def!(Tag u32 =>
    Comparator = 1,
    LogNumber = 2,
    NextFileNumber = 3,
    LastSequence = 4,
    CompactPointer = 5,
    DeletedFile = 6,
    NewFile = 7,
    PrevLogNumber = 9,
    MinLogNumberToKeep = 10,

//	kDbId = kTagSafeIgnoreMask + 1, // Encoded as a varlen string

    // RocksDB specific tags
    NewFile2 = 100,
    NewFile3 = 102,
    NewFile4 = 103,
    ColumnFamily = 200,
    ColumnFamilyAdd = 201,
    ColumnFamilyDrop = 202,
    MaxColumnFamily = 203,
    InAtomicGroup = 300
);

#[derive(Debug, Clone)]
pub struct NewFileEntry {
    pub level: u32,
    pub number: u64,
    pub file_size: u64,
    pub smallest_key: Vec<u8>,
    pub largest_key: Vec<u8>,

    /// Smallest and largest sequence numbers in the file (inclusive).
    /// Supported in NewFile2
    ///
    /// If we are iterating, then this could be used to skip entire tables if
    /// they are newer than the current snapshot.
    pub sequence_range: Option<(u64, u64)>,
}

impl NewFileEntry {
    pub fn compare(&self, other: &Self, key_comparator: &dyn KeyComparator) -> Ordering {
        // If the levels are not equal, then compare based on that.
        match self.level.cmp(&other.level) {
            Ordering::Equal => {}
            level_ord @ _ => {
                return level_ord;
            }
        };

        match key_comparator.compare(&self.smallest_key, &other.smallest_key) {
            Ordering::Equal => {}
            key_ord @ _ => {
                return key_ord;
            }
        };

        self.number.cmp(&other.number)
    }
}

#[derive(Debug)]
pub struct DeletedFileEntry {
    pub level: u32,
    pub number: u64,
}

#[derive(Default, Debug)]
pub struct VersionEdit {
    pub comparator: Option<String>,
    pub log_number: Option<u64>,
    pub prev_log_number: Option<u64>,
    pub last_sequence: Option<u64>,
    pub new_files: Vec<NewFileEntry>,
    pub deleted_files: Vec<DeletedFileEntry>,
    pub next_file_number: Option<u64>,
}

// TODO: Disallow file number re-use as it's likely to be error prone.

impl VersionEdit {
    // If a manifest gets too large, make a new one?

    /// Reads exactly one atomic VersionEdit from the given manifest file (or
    /// returns None if we reached the end of the file).
    pub async fn read(log: &mut RecordReader) -> Result<Option<Self>> {
        let record = match log.read().await? {
            Some(r) => r,
            None => {
                return Ok(None);
            }
        };

        let mut edit = VersionEdit::default();

        let mut input: &[u8] = record.as_ref();

        let mut touched_files = HashSet::new();

        // TODO: Verify that within the same record we don't create and delete the same
        // file.
        while input.len() > 0 {
            let record_id = Tag::from_value(parse_next!(input, parse_varint) as u32)?;

            // TODO: For most of these, don't allow reparsing if a value was
            // already found for the tag.
            match record_id {
                Tag::Comparator => {
                    let value = parse_next!(input, parse_string);
                    edit.comparator = Some(value);
                }
                Tag::LogNumber => {
                    let num = parse_next!(input, parse_varint);
                    edit.log_number = Some(num as u64);
                }
                Tag::LastSequence => {
                    let num = parse_next!(input, parse_varint);
                    edit.last_sequence = Some(num as u64);
                }
                Tag::NewFile => {
                    let level = parse_next!(input, parse_varint) as u32;
                    let number = parse_next!(input, parse_varint) as u64;
                    let file_size = parse_next!(input, parse_varint) as u64;
                    let smallest_key = parse_next!(input, parse_slice).to_vec();
                    let largest_key = parse_next!(input, parse_slice).to_vec();

                    if !touched_files.insert(number) {
                        return Err(err_msg(
                            "Not allowed to create the same new file in the same record",
                        ));
                    }

                    edit.new_files.push(NewFileEntry {
                        level,
                        number,
                        file_size,
                        smallest_key,
                        largest_key,
                        sequence_range: None,
                    });
                }
                Tag::NewFile2 => {
                    let level = parse_next!(input, parse_varint) as u32;
                    let number = parse_next!(input, parse_varint) as u64;
                    let file_size = parse_next!(input, parse_varint) as u64;
                    let smallest_key = parse_next!(input, parse_slice).to_vec();
                    let largest_key = parse_next!(input, parse_slice).to_vec();
                    let smallest_seq = parse_next!(input, parse_varint) as u64;
                    let largest_seq = parse_next!(input, parse_varint) as u64;
                    edit.new_files.push(NewFileEntry {
                        level,
                        number,
                        file_size,
                        smallest_key,
                        largest_key,
                        sequence_range: Some((smallest_seq, largest_seq)),
                    });
                }
                Tag::PrevLogNumber => {
                    let num = parse_next!(input, parse_varint) as u64;
                    // TODO:
                    edit.prev_log_number = Some(num);
                }
                Tag::NextFileNumber => {
                    let num = parse_next!(input, parse_varint) as u64;
                    edit.next_file_number = Some(num);
                }
                Tag::DeletedFile => {
                    let level = parse_next!(input, parse_varint) as u32;
                    let file_number = parse_next!(input, parse_varint) as u64;

                    if !touched_files.insert(file_number) {
                        return Err(err_msg("Deleted a file that was already referenced in the same VersionEdit is not allowed"));
                    }

                    edit.deleted_files.push(DeletedFileEntry {
                        level,
                        number: file_number,
                    });
                }
                _ => {
                    return Err(format_err!("Unsupported tag {:?}", record_id));
                }
            };
        }

        Ok(Some(edit))
    }

    /// Serializes
    pub fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {
        if let Some(comparator) = &self.comparator {
            serialize_varint(Tag::Comparator.to_value() as u64, out);
            serialize_string(comparator.as_str(), out);
        }

        if let Some(num) = &self.log_number {
            serialize_varint(Tag::LogNumber.to_value() as u64, out);
            serialize_varint(*num, out);
        }

        if let Some(num) = &self.prev_log_number {
            serialize_varint(Tag::PrevLogNumber.to_value() as u64, out);
            serialize_varint(*num, out);
        }

        if let Some(num) = &self.last_sequence {
            serialize_varint(Tag::LastSequence.to_value() as u64, out);
            serialize_varint(*num, out);
        }

        if let Some(num) = &self.next_file_number {
            serialize_varint(Tag::NextFileNumber.to_value() as u64, out);
            serialize_varint(*num, out);
        }

        let mut touched_files = HashSet::new();

        // NOTE: Deletions of files must be added before creation of new files as
        // compactions typically create new files that overlap with keys in old deleted
        // files so we must delete the old files first to avoid having multiple files in
        // the same level that are overlapping.
        //
        // RocksDB and LevelDB similarly add these to the manifest first.
        for file in &self.deleted_files {
            // Note that if we just inserting a new file in the same record then the
            // ordering of the change is undefined (different implementations may add all
            // the NewFile entries first or the DeletedFile entryies first).
            if !touched_files.insert(file.number) {
                return Err(err_msg("Duplicate file deletion"));
            }

            serialize_varint(Tag::DeletedFile.to_value() as u64, out);
            serialize_varint(file.level as u64, out);
            serialize_varint(file.number, out);
        }

        for file in &self.new_files {
            if !touched_files.insert(file.number) {
                return Err(err_msg("Created a file that was already touched"));
            }

            let tag = if file.sequence_range.is_some() {
                Tag::NewFile2
            } else {
                Tag::NewFile
            };
            serialize_varint(tag.to_value() as u64, out);

            serialize_varint(file.level as u64, out);
            serialize_varint(file.number, out);
            serialize_varint(file.file_size, out);
            serialize_slice(&file.smallest_key, out);
            serialize_slice(&file.largest_key, out);

            if let Some((min, max)) = file.sequence_range.clone() {
                serialize_varint(min, out);
                serialize_varint(max, out);
            }
        }

        Ok(())
    }
}

/*

Write Lock VersionEdit { comparator: None, log_number: Some(4), prev_log_number: Some(3), last_sequence: None, new_files: [], deleted_files: [], next_file_number: Some(5) }

Write Flush VersionEdit { comparator: None, log_number: None, prev_log_number: Some(0), last_sequence: Some(9000), new_files: [NewFileEntry { level: 2, number: 5, file_size: 24063, smallest_key: [49, 48, 48, 48, 1, 1, 0, 0, 0, 0, 0, 0], largest_key: [52, 51, 48, 53, 1, 234, 12, 0, 0, 0, 0, 0], sequence_range: None }], deleted_files: [], next_file_number: Some(6) }

Write Lock VersionEdit { comparator: None, log_number: Some(6), prev_log_number: Some(4), last_sequence: None, new_files: [], deleted_files: [], next_file_number: Some(7) }

Write Flush VersionEdit { comparator: None, log_number: None, prev_log_number: Some(0), last_sequence: Some(9000), new_files: [NewFileEntry { level: 2, number: 7, file_size: 40700, smallest_key: [52, 51, 48, 54, 1, 235, 12, 0, 0, 0, 0, 0], largest_key: [57, 57, 57, 57, 1, 40, 35, 0, 0, 0, 0, 0], sequence_range: None }], deleted_files: [], next_file_number: Some(8) }
*/
