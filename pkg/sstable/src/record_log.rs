/*
	Implementation of a sequential log format greatly inspired by Google's RecordIO / LevelDB Log Formats

	Right now the binary format is basically equivalent to the LevelDB format but hopefully we will add compression to it as well


	- This is meant to be used for any application needing an append only log
	- It should be resilient to crashes such that records that were only partially 
	- The general operations that this should support are:
		- Append new record to the end of the log
			- Also write a compressed record (or many tiny compressed records)
		- Read first record
		- Read last record
		- Find approximate record boundaries in a file and start a record read from that boundary
		- Iterate forwards or backwards from any readable record position


	References:
	- LevelDB's format is documented here
		- https://github.com/google/leveldb/blob/master/doc/log_format.md
	- RecordIO has been brief appearances in the file here:
		- See the Percolator/Caffeine papers
		- https://github.com/google/or-tools/blob/master/ortools/base/recordio.h
		- https://github.com/google/sling/blob/master/sling/file/recordio.h
		- https://github.com/google/trillian/blob/master/storage/tools/dump_tree/dumplib/dumplib.go
		- https://github.com/eclesh/recordio
		- https://news.ycombinator.com/item?id=16813030
		- https://github.com/google/riegeli

	TODO: Also 'ColumnIO' for columnar storage
*/


// TODO: Also useful would be to use fallocate on block sizes (or given a known maximum log size, we could utilize that size) 
// At the least, we can perform heuristics to preallocate for the current append at the least 

use common::block_size_remainder;
use common::errors::*;
use common::async_std::fs::{OpenOptions, File};
use common::async_std::io::{Read, Write, Seek, SeekFrom};
use common::async_std::io::prelude::{ReadExt, WriteExt, SeekExt};
use common::async_std::path::Path;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
//use byteorder::{WriteBytesExt, ReadBytesExt, LittleEndian};

//use std::io::{Read, Write, Cursor, Seek, SeekFrom};
//use std::fs::{OpenOptions, File};

const BLOCK_SIZE: u64 = 32*1024;

/// Number of bytes needed to represent just the header of a single record (same as in the LevelDB format)
const RECORD_HEADER_SIZE: u64 = 7;


enum_def!(RecordType u8 =>
	FULL = 1,
	FIRST = 2,
	MIDDLE = 3,
	LAST = 4
);

struct Record<'a> {
	checksum: u32,
	checksum_expected: u32,
	typ: RecordType,
	data: &'a [u8],
}

impl<'a> Record<'a> {
	/// Returns (parsed record, next offset in input)
	fn parse(input: &'a [u8]) -> Result<(Self, usize)> {
		if input.len() < (RECORD_HEADER_SIZE as usize) {
			return Err(err_msg("Input shorter than record header"));
		}

		let checksum = u32::from_le_bytes(*array_ref![input, 0, 4]);
		let length = u16::from_le_bytes(*array_ref![input, 4, 2]);
		let typ = RecordType::from_value(input[6])?;
		let data_start = RECORD_HEADER_SIZE as usize;
		let data_end = data_start + (length as usize);

		if input.len() < data_end {
			return Err(err_msg("Input smaller than data length"));
		}

		let data = &input[data_start..data_end];

		let checksum_expected = {
			let mut hasher = CRC32CHasher::new();
			// Hash [type, data]
			hasher.update(&input[6..data_end]);
			hasher.masked()
		};

		Ok((
			Self { checksum, checksum_expected, typ, data },
			data_end
		))
	}
}


pub struct RecordLog {
	file: File,

	file_size: u64,

	/// Current cursor into the file. This will be the offset at which the block
	/// buffer starts.
	file_offset: u64,

	/// Buffer containing up to a single block. May be smaller
	block: Vec<u8>,

	/// Next offset in the block offset to be read/written
	block_offset: usize

	// TODO: Must know if at the end of the file to know if we can start writing
	// (or consider the file offset to be at the start of the block )

	// TODO: Probably need to retain the last offset written without error to
	// ensure that we can truncate when there is invalid data.

	// TODO: Use a shared before when reading/writing

//	off: Option<u64>,
//	recs: Vec<Record<'a>>,
//	buf: Vec<u8> // [u8; BLOCK_SIZE],
}


impl RecordLog {

	pub async fn open(path: &Path, writeable: bool)
		-> Result<Self> {
		let file = OpenOptions::new()
			.read(true).write(writeable).open(path).await?;

		let file_size = file.metadata().await?.len();

		let mut block = vec![];
		block.reserve(BLOCK_SIZE as usize);

		Ok(Self { file, file_size, file_offset: 0, block, block_offset: 0 })
	}

	/*
	pub fn create(path: &Path) -> Result<Self> {
		let file = OpenOptions::new().create_new(true).read(true).write(true).open(path)?;

		// Seek to the last block ofset
		// Read it and truncate to last complete block in it
		// We assume all previous blocks are still valid
		// If first record chain in last block is not terminated, must seek backwards



	}
	*/

	// Generally should return the final position of the block
	// TODO: If we want to use this for error recovery, then it must be resilient to not reading enough of the file (basically bounds check the length given always (because even corruption in earlier blocks can have the same issue))
	// XXX: Also easier to verify the checksume right away


	/// Reads a complete block from the file starting at the given offset. After
	/// this is successful, the internal block buffer is use-able.
	async fn read_block(&mut self, off: u64) -> Result<()> {
		self.file.seek(SeekFrom::Start(off)).await?;

		let block_size = std::cmp::min(BLOCK_SIZE, self.file_size - off);

		self.block.resize(block_size as usize, 0);
		self.block_offset = 0;
		self.file_offset = off;

		self.file.read_exact(&mut self.block).await
			.map_err(|e| {
				// On error, clear the block so that it can't be used in an
				// inconsistent state.
				self.block.clear();
				e
			})?;

		Ok(())
	}

	// TODO: If there are multiple middle-blocks after each other in the same
	// block, we should error out.

	async fn read_record<'a>(&'a mut self) -> Result<Option<Record<'a>>> {
		if self.block.len() - self.block_offset
			< (RECORD_HEADER_SIZE as usize) {
			if self.file_offset + (self.block.len() as u64) >= self.file_size {
				return Ok(None)
			}

			self.read_block(self.file_offset + (self.block.len() as u64))
				.await?;
		}

		let (record, next_offset) = Record::parse(
			&self.block[self.block_offset..])?;
		self.block_offset += next_offset;

		if record.checksum_expected != record.checksum {
			return Err(err_msg("Checksum mismatch in record"));
		}

		Ok(Some(record))
	}

	pub async fn read(&mut self) -> Result<Option<Vec<u8>>> {
		let mut out = vec![];

		let first_record = match self.read_record().await? {
			Some(record) => record,
			None => { return Ok(None); }
		};

		if first_record.typ == RecordType::FULL {
			return Ok(Some(first_record.data.to_vec()));
		} else if first_record.typ == RecordType::FIRST {
			out.extend_from_slice(first_record.data);
		} else {
			return Err(err_msg("Unexpected initial record type"));
		}


		loop {
			let next_record = match self.read_record().await? {
				Some(record) => record,
				None => { return Err(err_msg("Incomplete user record")); }
			};

			out.extend_from_slice(next_record.data);

			match next_record.typ {
				RecordType::MIDDLE => { continue; },
				RecordType::LAST => { break; },
				_ => {
					return Err(err_msg(
						"Unexpected type in the middle of a user record"));
				}
			};
		}

		Ok(Some(out))
	}

	// TODO: Verify this code
	// TODO: Buffer all writes and have a separate flush() operation.
	pub async fn append(&mut self, data: &[u8]) -> Result<()> {

		let mut extent = self.file.seek(SeekFrom::End(0)).await?;

		// Must start in the next block if we can't fit at least a single
		// zero-length block in this block
		let rem = block_size_remainder(BLOCK_SIZE, extent);
		if rem < RECORD_HEADER_SIZE {
			extent += rem;
			self.file.set_len(extent).await?;
			self.file.seek(SeekFrom::End(0)).await?;
		}

		let mut header = [0u8; RECORD_HEADER_SIZE as usize];

		let mut pos = data.len();
		while pos < data.len() {

			let rem = block_size_remainder(BLOCK_SIZE, extent)
				- RECORD_HEADER_SIZE;
			let take = std::cmp::min(rem as usize, (data.len() - pos));

			let typ =
				if pos == 0 {
					if take == data.len() { RecordType::FULL }
					else { RecordType::FIRST }
				} else {
					if pos + take == data.len() { RecordType::LAST }
					else { RecordType::MIDDLE }
				} as u8;

			// Checksum of [ type, data ]
			let sum = {
				let mut hasher = CRC32CHasher::new();
				hasher.update(std::slice::from_ref(&typ));
				hasher.update(data);
				hasher.masked()
			};

			header[0..4].copy_from_slice(&sum.to_le_bytes());
			header[4..6].copy_from_slice(&(take as u16).to_le_bytes());
			header[6] = typ;

			self.file.write_all(&header).await?;
			self.file.write_all(&data[pos..(pos + take)]).await?;
			pos += take;
		}

		Ok(())
	}


}


