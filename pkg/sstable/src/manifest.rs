use common::errors::*;
use protobuf::wire::{parse_varint};
use crate::record_log::RecordLog;
use crate::encoding::*;

// See https://github.com/facebook/rocksdb/blob/5f025ea8325a2ff5239ea28365073bf0b723514d/db/version_edit.cc#L29 for the complete list of tags
// Also https://github.com/google/leveldb/blob/master/db/version_edit.cc

// NOTE: A seuqnece number is a u64

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

#[derive(Debug)]
pub struct NewFileEntry {
	pub level: u32,
	pub number: u64,
	pub file_size: u64,
	pub smallest_key: Vec<u8>,
	pub largest_key: Vec<u8>,
	/// Smallest and largest sequence numbers in the file (inclusive).
	pub sequence_range: Option<(u64, u64)>
}

#[derive(Debug)]
struct DeletedFileEntry {
	pub level: u32,
	pub number: u64
}


#[derive(Default, Debug)]
pub struct VersionEdit {
	pub comparator: Option<String>,
	pub log_number: Option<u64>,
	pub prev_log_number: Option<u64>,
	pub last_sequence: Option<u64>,
	pub new_files: Vec<NewFileEntry>,
	pub next_file_number: Option<u64>
}

impl VersionEdit {
	pub async fn read(log: &mut RecordLog) -> Result<Self> {
		let mut edit = VersionEdit::default();

		while let Some(record) = log.read().await? {
			let mut input: &[u8] = record.as_ref();

			while input.len() > 0 {
				let record_id = Tag::from_value(
					parse_next!(input, parse_varint) as u32)?;

				// TODO: For most of these, don't allow reparsing if a value was
				// already found for the tag.
				match record_id {
					Tag::Comparator => {
						let value = parse_next!(input, parse_string);
						edit.comparator = Some(value);
					},
					Tag::LogNumber => {
						let num = parse_next!(input, parse_varint);
						edit.log_number = Some(num as u64);
					},
					Tag::LastSequence => {
						let num = parse_next!(input, parse_varint);
						edit.last_sequence = Some(num as u64);
					},
					Tag::NewFile => {
						let level = parse_next!(input, parse_varint) as u32;
						let number = parse_next!(input, parse_varint) as u64;
						let file_size = parse_next!(input, parse_varint) as u64;
						let smallest_key = parse_next!(input, parse_slice).to_vec();
						let largest_key = parse_next!(input, parse_slice).to_vec();

						edit.new_files.push(NewFileEntry {
							level, number, file_size, smallest_key, largest_key,
							sequence_range: None
						});
					},
					Tag::NewFile2 => {
						let level = parse_next!(input, parse_varint) as u32;
						let number = parse_next!(input, parse_varint) as u64;
						let file_size = parse_next!(input, parse_varint) as u64;
						let smallest_key = parse_next!(input, parse_slice).to_vec();
						let largest_key = parse_next!(input, parse_slice).to_vec();
						let smallest_seq = parse_next!(input,
													   parse_varint) as u64;
						let largest_seq = parse_next!(input,
													  parse_varint) as u64;
						edit.new_files.push(NewFileEntry {
							level, number, file_size, smallest_key, largest_key,
							sequence_range: Some((smallest_seq, largest_seq))
						});
					},
					Tag::PrevLogNumber => {
						let num = parse_next!(input, parse_varint) as u64;
						edit.prev_log_number = Some(num);
					},
					Tag::NextFileNumber => {
						let num = parse_next!(input, parse_varint) as u64;
						edit.next_file_number = Some(num);
					},
					_ => {
						return Err(format!("Unsupported tag {:?}",
										   record_id).into());
					}
				};
			}
		}

		Ok(edit)
	}

}
