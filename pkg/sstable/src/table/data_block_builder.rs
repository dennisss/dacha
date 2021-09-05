use std::collections::BTreeMap;

use common::errors::*;

use crate::table::data_block::{DataBlockEntry, DataBlockRef};

/// Builds a key-value style block.
///
/// A data block may have zero keys in which case it will simply be serialized
/// with no data but a single restart at offset 0.
pub struct DataBlockBuilder {
    restart_interval: usize,
    buffer: Vec<u8>,
    restart_offsets: Vec<u32>,
    entries_since_restart: usize,
    last_key: Vec<u8>,
}

impl DataBlockBuilder {
    pub fn new(restart_interval: usize) -> Self {
        Self {
            restart_interval,
            buffer: vec![],
            restart_offsets: vec![0],
            entries_since_restart: 0,
            last_key: vec![],
        }
    }

    pub fn empty(&self) -> bool {
        self.buffer.len() == 0
    }

    pub fn current_size(&self) -> usize {
        self.buffer.len()
    }

    /// Expected size of the current block after adding the given row.
    pub fn projected_size(&self, key: &[u8], value: &[u8]) -> usize {
        self.buffer.len() + key.len() + value.len() + 4 * self.restart_offsets.len()
    }

    pub fn add(&mut self, key: &[u8], value: &[u8]) -> Result<()> {
        // Keys must be inserted strictly in sorted order.
        // TODO: Use the comparator here.
        // TODO: Add back this check.
        // if key <= &self.last_key {
        //     return Err(err_msg("Out of order or duplicate key inserted"));
        // }

        let mut shared_bytes = 0;

        // Check if we should restart prefix compression.
        if self.entries_since_restart >= self.restart_interval {
            self.restart_offsets.push(self.buffer.len() as u32);
        } else {
            while shared_bytes < std::cmp::min(self.last_key.len(), key.len()) {
                if self.last_key[shared_bytes] != key[shared_bytes] {
                    break;
                }

                shared_bytes += 1;
            }
        }

        DataBlockEntry {
            shared_bytes: shared_bytes as u32,
            key_delta: &key[shared_bytes..],
            value,
        }
        .serialize(&mut self.buffer);

        self.entries_since_restart += 1;
        self.last_key.clear();
        self.last_key.extend_from_slice(key);

        Ok(())
    }

    /// TODO: Split this struct into a Block and BlockFooter
    /// NOTE: This does NOT serialize the entries.
    fn serialize_block_footer(hash_index: Option<&[u8]>, restarts: &[u32], output: &mut Vec<u8>) {
        // TODO: Never include a hash index if the table is empty.
        if let Some(buckets) = hash_index {
            assert!(buckets.len() <= 255);
            output.extend_from_slice(buckets);
            output.push(buckets.len() as u8);
        }

        output.reserve(restarts.len() * 4);
        for r in restarts {
            output.extend_from_slice(&r.to_le_bytes());
        }

        let mut packed = restarts.len() as u32;
        if hash_index.is_some() {
            packed |= 1 << 31;
        }

        output.extend_from_slice(&packed.to_le_bytes());
    }

    /// Writes the footer for the block and returns the complete block.
    /// After calling this the builder is in a reset state and can be used for
    /// building different blocks.
    pub fn finish(&mut self) -> (Vec<u8>, Vec<u8>) {
        // TODO: What should the data representation be for an empty block?
        // e.g. the metaindex could be empty if no filter is configured.
        // - Need to test that we can encode and decode an empty block.

        Self::serialize_block_footer(None, &self.restart_offsets, &mut self.buffer);

        self.restart_offsets.clear();
        self.restart_offsets.push(0);

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
pub struct UnsortedDataBlockBuilder {
    restart_interval: usize,
    data: Vec<(Vec<u8>, Vec<u8>)>,
}

impl UnsortedDataBlockBuilder {
    pub fn new(restart_interval: usize) -> Self {
        Self {
            restart_interval,
            data: vec![],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.len() == 0
    }

    pub fn add(&mut self, key: Vec<u8>, value: Vec<u8>) {
        self.data.push((key, value));
    }

    pub fn finish(&mut self) -> Result<Vec<u8>> {
        let mut builder = DataBlockBuilder::new(self.restart_interval);

        // TODO: Use an appropriate comparator here.
        self.data.sort_unstable();

        for (key, value) in self.data.iter() {
            builder.add(&key, &value)?;
        }

        Ok(builder.finish().0)
    }
}
