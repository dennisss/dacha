use common::errors::*;
use crate::block::{BlockEntry, Block};
use std::collections::BTreeMap;

/// Builds a key-value style block.
pub struct BlockBuilder {
	restart_interval: usize,
	buffer: Vec<u8>,
	restart_offsets: Vec<u32>,
	entries_since_restart: usize,
	last_key: Vec<u8>
}

impl BlockBuilder {
	pub fn new(restart_interval: usize) -> Self {
		Self {
			restart_interval,
			buffer: vec![],
			restart_offsets: vec![],
			entries_since_restart: 0,
			last_key: vec![]
		}
	}

	pub fn empty(&self) -> bool { self.buffer.len() == 0 }

	pub fn current_size(&self) -> usize { self.buffer.len() }

	/// Expected size of the current block after adding the given row.
	pub fn projected_size(&self, key: &[u8], value: &[u8]) -> usize {
		self.buffer.len() + key.len() + value.len()
			+ 4*self.restart_offsets.len()
	}

	pub fn add(&mut self, key: Vec<u8>, value: Vec<u8>) -> Result<()> {
		// Keys must be inserted strictly in sorted order.
		if key <= self.last_key {
			return Err("Out of order or duplicate key inserted".into());
		}

		let mut shared_bytes = 0;

		// Check if we should restart prefix compression.
		if self.buffer.len() == 0 ||
			self.entries_since_restart >= self.restart_interval {
			self.restart_offsets.push(self.buffer.len() as u32);
		} else {
			while shared_bytes < std::cmp::min(self.last_key.len(), key.len()) {
				if self.last_key[shared_bytes] != key[shared_bytes] {
					break;
				}

				shared_bytes += 1;
			}
		}

		BlockEntry {
			shared_bytes: shared_bytes as u32,
			key_delta: &key[shared_bytes..],
			value: &value
		}.serialize(&mut self.buffer);

		self.entries_since_restart += 1;
		self.last_key = key;

		Ok(())
	}


	/// Writes the footer for the block and returns the complete block.
	/// After calling this the builder is in a reset state and can be used for
	/// building different blocks.
	pub fn finish(&mut self) -> (Vec<u8>, Vec<u8>) {
		Block {
			entries: &[], hash_index: None, restarts: &self.restart_offsets
		}.serialize(&mut self.buffer);

		self.restart_offsets.clear();
		self.entries_since_restart = 0;

		(self.buffer.split_off(0), self.last_key.split_off(0))
	}

	/// Specifies a buffer to use internally. Typically this will be called
	/// after finish() with the returned buffer to allow reusing a single block
	/// of memory
	pub fn set_buffer(&mut self, mut buffer: Vec<u8>) {
		assert_eq!(self.buffer.len(), 0);
		buffer.clear();
		self.buffer = buffer;
	}
}

/// A block builder that allows inserting keys in unsorted order.
/// NOTE: This is mainly for internal usage to implement meta/index blocks and
/// won't help you build data blocks as no ordering constrains are made between
/// separate blocks.
///
/// This also doesn't have as many buffer re-use optimizations as BlockBuilder
/// does.
pub struct UnsortedBlockBuilder {
	restart_interval: usize,
	data: Vec<(Vec<u8>, Vec<u8>)>
}

impl UnsortedBlockBuilder {
	pub fn new(restart_interval: usize) -> Self {
 		Self { restart_interval, data: vec![] }
	}

	pub fn empty(&self) -> bool { self.data.len() == 0 }

	pub fn add(&mut self, key: Vec<u8>, value: Vec<u8>) {
		self.data.push((key, value));
	}

	pub fn finish(mut self) -> Result<Vec<u8>> {
		let mut builder = BlockBuilder::new(self.restart_interval);

		self.data.sort_unstable();
		for (key, value) in self.data.into_iter() {
			builder.add(key, value)?;
		}

		Ok(builder.finish().0)
	}

}
