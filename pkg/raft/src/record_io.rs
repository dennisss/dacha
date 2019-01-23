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


use std::io::{Read, Write, Cursor, Seek};
use std::fs::{OpenOptions, File, SeekFrom};
use std::path::Path;
use crc32c::crc32c_append;
use byteorder::{WriteBytesExt, ReadBytesExt, LittleEndian};
use core::block_size_remainder;

const BLOCK_SIZE: u64 = 32*1024;

/// Number of bytes needed to represent just the header of a single record (same as in the LevelDB format)
const RECORD_HEADER_SIZE: u64 = 7;


enum RecordType {
	FULL = 1,
	FIRST = 2,
	MIDDLE = 3,
	LAST = 4
}

struct Record<'a> {
	checksum: u32,
	type: u8,
	data: &'a [u8]
}



pub struct RecordIO<'a> {
	file: File,
	
	off: Option<u64>,
	recs: Vec<Record<'a>>
	buf: [0u8; BLOCK_SIZE],


}


impl<'a> RecordIO<'a> {

	pub fn open(path: &Path) -> Result<Self> {
		let file = OpenOptions::new().read(true).write(true).open(path)?;

	}

	pub fn create(path: &Path) -> Result<Self> {
		let file = OpenOptions::new().create_new(true).read(true).write(true).open(path)?;

		// Seek to the last block ofset
		// Read it and truncate to last complete block in it
		// We assume all previous blocks are still valid
		// If first record chain in last block is not terminated, must seek backwards



	}

	// Generally should return the final position of the block
	// TODO: If we want to use this for error recovery, then it must be resilient to not reading enough of the file (basically bounds check the length given always (because even corruption in earlier blocks can have the same issue))
	// XXX: Also easier to verify the checksume right away
	fn read_block(&mut self, off: u64) -> Result<()> {
		self.off = None;
		self.recs.clear();

		self.file.seek(SeekFrom::Start(off))?;
		
		let n = self.file.read(&mut self.buf)?;

		let mut cur = Cursor::new(&self.buf);

		let mut i = 0;

		while i + RECORD_HEADER_SIZE <= n {



		}


		self.off = Some(off);
	}

	pub fn append(&mut self, data: &[u8]) -> Result<()> {

		let extent = self.file.seek(pos: SeekFrom::End(0))?;

		// Must start in the next block if we can't fit at least a single zero-length block in this block
		let rem = block_size_remainder(BLOCK_SIZE, pos);
		if rem < RECORD_HEADER_SIZE {
			extent += rem;
			self.file.set_len(extent)?;
			self.file.seek(SeekFrom::End(0))?;
		}

		let mut header = [0u8; RECORD_HEADER_SIZE];

		let mut pos = data.len();
		while pos < data.len() {

			let rem = block_size_remainder(BLOCK_SIZE, extent) - RECORD_HEADER_SIZE;
			let take = std::cmp::min(rem, data.len() - pos);

			let type =
				if pos == 0 {
					if take == data.len() { RecordType::FULL }
					else { RecordType::FIRST }
				} else {
					if pos + take == data.len() { RecordType::LAST }
					else { RecordType::MIDDLE }
				} as u8;

			// Checksum of [ type, data ]
			let sum = crc32c_append(crc32c_append(0, slice::from_ref(&type)), data);

			(&header[0..4]).write_u32::<LittleEndian>(sum)?;
			(&header[4..6]).write_u16::<LittleEndian>(take)?;
			header[6] = type;

			self.file.write_all(&header)?;
			self.file.write_all(data[pos..(pos + take)])?;
			pos += take;
		}

		Ok(())
	}


}


