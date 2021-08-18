// This file contains the implementation of a 'Block' which is a container of
// many prefix-compressed entries representing key-value pairs. It should not
// be confused with meta blocks which may have a different format.

use std::cmp::Ordering;

use common::errors::*;
use parsing::*;
use protobuf::wire::{parse_varint, serialize_varint};

use crate::encoding::u32_slice;

// TODO: The hash-based index is stored before the list of resets here:
// https://github.com/facebook/rocksdb/blob/50e470791dafb3db017f055f79323aef9a607e43/table/block_based/block_builder.cc#L118

// DataBlockHashIndexBuilder format is defined here: https://github.com/facebook/rocksdb/blob/50e470791dafb3db017f055f79323aef9a607e43/table/block_based/data_block_hash_index.h#L14

// TODO: Implement usage of this hash index.
pub const HASH_INDEX_NO_ENTRY: u8 = 255;
pub const HASH_INDEX_COLLISION: u8 = 254;

/// A block containing key-value pairs. This is the format used by data blocks,
/// the meta-index block, and the index block.
#[derive(Debug)]
pub struct Block<'a> {
    /// Serialized BlockEntry structs containing the key-value data.
    pub entries: &'a [u8],

    pub hash_index: Option<&'a [u8]>,

    /// TODO: Is it even required to specify zero as a restart?
    pub restarts: &'a [u32],
}

impl<'a> Block<'a> {
    pub fn parse(mut input: &'a [u8]) -> Result<Self> {
        min_size!(input, 4);

        let packed = u32::from_le_bytes(*array_ref![input, input.len() - 4, 4]);
        input = &input[0..(input.len() - 4)];

        let index_type = if (packed >> 31) == 0 {
            BlockIndexType::BinarySearch
        } else {
            BlockIndexType::BinaryAndHash
        };
        let num_restarts = (packed & !(1 << 31)) as usize;

        if num_restarts < 1 {
            return Err(err_msg("At least one restart is required"));
        }

        let hash_index = if index_type == BlockIndexType::BinaryAndHash {
            min_size!(input, 1);
            let num_buckets = input[input.len() - 1] as usize;
            input = &input[..(input.len() - 1)];

            min_size!(input, num_buckets);
            let split = input.split_at(input.len() - num_buckets);
            input = split.0;
            Some(split.1)
        } else {
            None
        };

        min_size!(input, 4 * num_restarts);
        let restarts = {
            let split = input.split_at(input.len() - 4 * num_restarts);
            input = split.0;
            u32_slice(split.1)
        };

        Ok(Self {
            entries: input,
            hash_index,
            restarts,
        })
    }

    /// TODO: Split this struct into a Block and BlockFooter
    /// NOTE: This does NOT serialize the entries.
    pub fn serialize(&self, output: &mut Vec<u8>) {
        if let Some(buckets) = self.hash_index {
            assert!(buckets.len() <= 255);
            output.extend_from_slice(buckets);
            output.push(buckets.len() as u8);
        }

        output.reserve(self.restarts.len() * 4);
        for r in self.restarts {
            output.extend_from_slice(&r.to_le_bytes());
        }

        let mut packed = self.restarts.len() as u32;
        if self.hash_index.is_some() {
            packed |= 1 << 31;
        }

        output.extend_from_slice(&packed.to_le_bytes());
    }

    /// Retrieves a single key-value pair by key.
    /// Compared to using an iterator, this may use more optimizations for point
    /// lookups.
    pub fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        // TODO: Implement hash-based lookup

        let mut iter = self.before(key)?.rows();
        while let Some(kv) = iter.next() {
            let kv = kv?;
            match key.cmp(kv.key) {
                Ordering::Equal => {
                    return Ok(Some(iter.last_key()));
                }
                Ordering::Less => {
                    continue;
                }
                Ordering::Greater => {
                    break;
                }
            }
        }

        Ok(None)
    }

    /// Creates an iterator that starts at the beginning of the block.
    pub fn iter(&'a self) -> BlockEntryIterator<'a> {
        BlockEntryIterator {
            remaining_entries: self.entries,
        }
    }

    /// Creates an iterator that begins with keys <= the given key where the
    /// first key seen is as close as possible to the given key.
    ///
    /// If the given key is not in the table, then the iterator may start after
    /// the given key.
    pub fn before(&'a self, key: &[u8]) -> Result<BlockEntryIterator<'a>> {
        let closest_offset = self.restart_search(key, self.restarts)?;
        Ok(BlockEntryIterator {
            remaining_entries: &self.entries[closest_offset..],
        })
    }

    /// NOTE: This assumes that restarts has a length of at least 1.
    // TODO: This will perform redundant entry parsing with the iterator.
    // ^ Possibly pre-parse all of the restart points?
    fn restart_search(&self, key: &[u8], restarts: &[u32]) -> Result<usize> {
        if restarts.len() == 1 {
            return Ok(restarts[0] as usize);
        }

        let mid_index = restarts.len() / 2;
        let mid_offset = restarts[mid_index] as usize;
        let (mid_entry, _) = BlockEntry::parse(&self.entries[mid_offset..])?;
        if mid_entry.shared_bytes != 0 {
            return Err(err_msg("Restart not valid"));
        }

        // TODO: Refactor to be non-recursive.
        match key.cmp(mid_entry.key_delta) {
            Ordering::Equal => Ok(mid_offset as usize),
            Ordering::Less => self.restart_search(key, &restarts[..mid_index]),
            Ordering::Greater => self.restart_search(key, &restarts[mid_index..]),
        }
    }
}

#[derive(PartialEq)]
pub enum BlockIndexType {
    BinarySearch,
    BinaryAndHash,
}

#[derive(Debug)]
pub struct BlockEntry<'a> {
    /// Number of prefix bytes from the last entry's key which are the same as
    /// the key for the current entry.
    pub shared_bytes: u32,

    /// Additional unique key bytes for this entry coming after the shared ones.
    pub key_delta: &'a [u8],

    /// The complete value associated with this key.
    pub value: &'a [u8],
}

impl<'a> BlockEntry<'a> {
    pub fn parse(mut input: &'a [u8]) -> Result<(Self, &'a [u8])> {
        let shared_bytes = parse_next!(input, parse_varint) as usize;
        let unshared_bytes = parse_next!(input, parse_varint) as usize;
        let value_length = parse_next!(input, parse_varint) as usize;

        min_size!(input, unshared_bytes + value_length);
        let (key_delta, input) = input.split_at(unshared_bytes);
        let (value, input) = input.split_at(value_length);

        Ok((
            Self {
                shared_bytes: shared_bytes as u32,
                key_delta,
                value,
            },
            input,
        ))
    }

    pub fn serialize(&self, out: &mut Vec<u8>) {
        serialize_varint(self.shared_bytes as u64, out);
        serialize_varint(self.key_delta.len() as u64, out); // unshared_bytes: u32
        serialize_varint(self.value.len() as u64, out); // value_length: u32
        out.extend_from_slice(self.key_delta);
        out.extend_from_slice(self.value);
    }
}

/// Iterator over raw entries in a block.
pub struct BlockEntryIterator<'a> {
    /// The remaining un-parsed entry data. This is a sub-slice of
    /// Block::entries.
    remaining_entries: &'a [u8],
}

impl<'a> BlockEntryIterator<'a> {
    pub fn rows(self) -> BlockKeyValueIterator<'a> {
        BlockKeyValueIterator {
            inner: self,
            last_key: vec![],
        }
    }
}

impl<'a> Iterator for BlockEntryIterator<'a> {
    type Item = Result<BlockEntry<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_entries.len() == 0 {
            return None;
        }

        Some(match BlockEntry::parse(self.remaining_entries) {
            Ok((entry, rest)) => {
                self.remaining_entries = rest;
                Ok(entry)
            }
            Err(e) => Err(e),
        })
    }
}

pub struct BlockKeyValueIterator<'a> {
    inner: BlockEntryIterator<'a>,
    last_key: Vec<u8>,
}

impl BlockKeyValueIterator<'_> {
    pub fn next(&mut self) -> Option<Result<KeyValuePair>> {
        let entry = match self.inner.next() {
            Some(Ok(v)) => v,
            Some(Err(e)) => {
                return Some(Err(e));
            }
            None => {
                return None;
            }
        };

        if entry.shared_bytes as usize > self.last_key.len() {
            return Some(Err(err_msg("Insufficient shared bytes from previous key.")));
        }

        self.last_key.truncate(entry.shared_bytes as usize);
        self.last_key.extend_from_slice(entry.key_delta);

        Some(Ok(KeyValuePair {
            key: &self.last_key,
            value: entry.value,
        }))
    }

    pub fn last_key(self) -> Vec<u8> {
        self.last_key
    }
}

pub struct KeyValuePair<'a> {
    pub key: &'a [u8],
    pub value: &'a [u8],
}
