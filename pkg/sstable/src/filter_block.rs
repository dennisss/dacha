

// TODO: Must parse internal keys to user keys during bloom filter construction: https://github.com/google/leveldb/blob/863f185970eff21e826e5fe1164a6215a515c23b/db/dbformat.h#L102
// ^ Both the comparators and filter policies in LevelDB wrap it

// This is the FilterPolicy definition: https://github.com/google/leveldb/blob/c784d63b931d07895833fb80185b10d44ad63cce/include/leveldb/filter_policy.h#L27

use common::errors::*;
use std::sync::Arc;
use crate::encoding::u32_slice;

pub trait FilterPolicy {
	fn name(&self) -> &'static str;
	fn create(&self, keys: Vec<&[u8]>, out: &mut Vec<u8>);
	fn key_may_match(&self, key: &[u8], filter: &[u8]) -> bool;
}

pub struct FilterBlock<'a> {
	/// Buffer containing all filter sub-blocks. They are delimited by the
	/// offsets.
	filters: &'a [u8],
	/// Offset of each filter in the filters array.
	/// NOTE: These have been checked at parse time to be in range.
	offsets: &'a [u32],
	log_base: usize,
}

impl<'a> FilterBlock<'a> {
	pub fn parse(input: &'a [u8]) -> Result<Self> {
		min_size!(input, 4 + 1);
		let log_base = input[input.len() - 1] as usize;
		let footer_start = input.len() - 5;
		let offsets_offset = u32::from_le_bytes(
			*array_ref![input, footer_start, 4]) as usize;

		if offsets_offset > footer_start {
			return Err("Out of range offsets array".into());
		}

		let offsets = {
			// NOTE: The last offset is the same as offsets_offset for
			// convenience.
			let buf = &input[offsets_offset..(input.len() - 1)];
			if buf.len() % 4 != 0 {
				return Err("Misaligned offsets array".into());
			}

			u32_slice(buf)
		};

		// The first filter should always start at the beginning of the block.
		// Otherwise there is data that we do not understand in the block.
		if offsets.len() == 0 {
			if offsets_offset != 0 {
				return Err("Unknown data before offsets array".into());
			}
		} else if offsets[0] != 0 {
			return Err("First filter does not start at zero".into());
		}

		// Check that all offsets are in range.
		for off in offsets {
			if (*off as usize) > offsets_offset {
				return Err("Out of range filter offset".into());
			}
		}

		let filters = &input[..offsets_offset];

		Ok(Self { filters, offsets, log_base })
	}

	pub fn key_may_match(&self, policy: &dyn FilterPolicy,
						 block_offset: usize, key: &'a [u8]) -> bool {
		let filter_idx = block_offset >> self.log_base;
		// NOTE: The very last offset is the end marker after all filters.
		if filter_idx >= self.offsets.len() - 1 {
			// No filter is present for the given block.
			return true;
		}

		let start_offset = self.offsets[filter_idx] as usize;
		let end_offset = self.offsets[filter_idx + 1] as usize;

		policy.key_may_match(key, &self.filters[start_offset..end_offset])
	}
}
