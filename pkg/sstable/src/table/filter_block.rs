// TODO: Must parse internal keys to user keys during bloom filter construction: https://github.com/google/leveldb/blob/863f185970eff21e826e5fe1164a6215a515c23b/db/dbformat.h#L102
// ^ Both the comparators and filter policies in LevelDB wrap it

// This is the FilterPolicy definition: https://github.com/google/leveldb/blob/c784d63b931d07895833fb80185b10d44ad63cce/include/leveldb/filter_policy.h#L27

use std::sync::Arc;

use common::errors::*;
use file::LocalFile;

use crate::encoding::u32_slice;
use crate::table::block_handle::BlockHandle;
use crate::table::filter_policy::*;

use super::footer::Footer;
use super::raw_block::RawBlock;

pub struct FilterBlock {
    block: Vec<u8>,
    block_ref: FilterBlockRef<'static>,
}

impl FilterBlock {
    pub async fn read(
        file: &mut LocalFile,
        footer: &Footer,
        block_handle: &BlockHandle,
    ) -> Result<Self> {
        let block = RawBlock::read(file, footer, block_handle)
            .await?
            .decompress()?;
        let block_ref = FilterBlockRef::parse(&block)?;

        // Make 'static
        let block_ref = unsafe { std::mem::transmute(block_ref) };

        Ok(Self { block, block_ref })
    }

    pub fn block<'a>(&'a self) -> &'a FilterBlockRef<'a> {
        &self.block_ref
    }
}

pub struct FilterBlockRef<'a> {
    /// Buffer containing all filter sub-blocks. They are delimited by the
    /// offsets.
    filters: &'a [u8],
    /// Offset of each filter in the filters array.
    /// NOTE: These have been checked at parse time to be in range.
    offsets: &'a [u32],
    log_base: usize,
}

impl<'a> FilterBlockRef<'a> {
    pub fn parse(input: &'a [u8]) -> Result<Self> {
        min_size!(input, 4 + 1);
        let log_base = input[input.len() - 1] as usize;
        let footer_start = input.len() - 5;
        let offsets_offset = u32::from_le_bytes(*array_ref![input, footer_start, 4]) as usize;

        if offsets_offset > footer_start {
            return Err(err_msg("Out of range offsets array"));
        }

        let offsets = {
            // NOTE: The last offset is the same as offsets_offset for
            // convenience.
            let buf = &input[offsets_offset..(input.len() - 1)];
            if buf.len() % 4 != 0 {
                return Err(err_msg("Misaligned offsets array"));
            }

            u32_slice(buf)
        };

        // The first filter should always start at the beginning of the block.
        // Otherwise there is data that we do not understand in the block.
        if offsets.len() == 0 {
            if offsets_offset != 0 {
                return Err(err_msg("Unknown data before offsets array"));
            }
        } else if offsets[0] != 0 {
            return Err(err_msg("First filter does not start at zero"));
        }

        // Check that all offsets are in range.
        for off in offsets {
            if (*off as usize) > offsets_offset {
                return Err(err_msg("Out of range filter offset"));
            }
        }

        let filters = &input[..offsets_offset];

        Ok(Self {
            filters,
            offsets,
            log_base,
        })
    }

    pub fn key_may_match(
        &self,
        policy: &dyn FilterPolicy,
        block_offset: usize,
        key: &[u8],
    ) -> bool {
        // TODO: Check ahead of time that there aren't more filters than blocks in the
        // table.

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
