// This file contains the implementation of a 'Block' which is a container of
// many prefix-compressed entries representing key-value pairs. It should not
// be confused with meta blocks which may have a different format.

use std::cmp::Ordering;
use std::sync::Arc;

use common::async_std::fs::File;
use common::errors::*;
use parsing::*;
use protobuf::wire::{parse_varint, serialize_varint};

use crate::encoding::u32_slice;
use crate::table::block_handle::BlockHandle;
use crate::table::footer::Footer;
use crate::table::raw_block::RawBlock;

use super::comparator::KeyComparator;

// TODO: The hash-based index is stored before the list of resets here:
// https://github.com/facebook/rocksdb/blob/50e470791dafb3db017f055f79323aef9a607e43/table/block_based/block_builder.cc#L118

// DataBlockHashIndexBuilder format is defined here: https://github.com/facebook/rocksdb/blob/50e470791dafb3db017f055f79323aef9a607e43/table/block_based/data_block_hash_index.h#L14

// TODO: Implement usage of this hash index.
pub const HASH_INDEX_NO_ENTRY: u8 = 255;
pub const HASH_INDEX_COLLISION: u8 = 254;

/// NOTE: After creation, a DataBlock is immutable.
/// TODO: Try doing something like https://stackoverflow.com/questions/23743566/how-can-i-force-a-structs-field-to-always-be-immutable-in-rust to force the immutability. (in particular, the Vec<u8> should never be allowed to be moved.
pub struct DataBlock {
    uncompressed: Vec<u8>,
    block: DataBlockRef<'static>,
}

impl DataBlock {
    /// TODO: For the index, we don't need the Arc as we will immediately cast
    /// to a different format.
    pub async fn read(file: &mut File, footer: &Footer, handle: &BlockHandle) -> Result<Arc<Self>> {
        let raw = RawBlock::read(file, footer, handle).await?;
        let data = raw.decompress()?;
        let block = Self::parse(data)?;
        Ok(block)
    }

    /// NOTE: This is the only safe way to create a DataBlock
    pub fn parse(data: Vec<u8>) -> Result<Arc<Self>> {
        let ptr: &'static [u8] = unsafe { std::mem::transmute::<&[u8], _>(&data) };
        let block = DataBlockRef::parse(ptr)?;
        Ok(Arc::new(Self {
            uncompressed: data,
            block,
        }))
    }

    /// NOTE: The addition of the lifetime is required to ensure that the
    /// internal static lifetime isn't leaked.
    pub fn block<'a>(&'a self) -> &'a DataBlockRef<'a> {
        &self.block
    }

    ///
    pub fn estimated_memory_usage(&self) -> usize {
        self.uncompressed.len()
    }
}

/// A block containing key-value pairs. This is the format used by data blocks,
/// the meta-index block, and the index block.
#[derive(Debug)]
pub struct DataBlockRef<'a> {
    /// Serialized BlockEntry structs containing the key-value data.
    entries: &'a [u8],

    hash_index: Option<&'a [u8]>,

    /// NOTE: The zero restart will always have an offset of 0.
    restarts: &'a [u32],
}

impl<'a> DataBlockRef<'a> {
    pub fn parse(mut input: &'a [u8]) -> Result<Self> {
        min_size!(input, 4);

        let packed = u32::from_le_bytes(*array_ref![input, input.len() - 4, 4]);
        input = &input[0..(input.len() - 4)];

        let index_type = if (packed >> 31) == 0 {
            DataBlockIndexType::BinarySearch
        } else {
            DataBlockIndexType::BinaryAndHash
        };
        let num_restarts = (packed & !(1 << 31)) as usize;

        if num_restarts < 1 {
            return Err(err_msg("At least one restart is required"));
        }

        let hash_index = if index_type == DataBlockIndexType::BinaryAndHash {
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

        if restarts[0] != 0 {
            return Err(err_msg(
                "Expected blocks to always have a restart at offset 0",
            ));
        }

        Ok(Self {
            entries: input,
            hash_index,
            restarts,
        })
    }

    /*
    // TODO: Before we use this, we need to extract the user key and use that for comparisons.
    // Also we need to use the user key for the hash index.

    /// Retrieves a single key-value pair by key.
    /// Compared to using an iterator, this may use more optimizations for point
    /// lookups.
    pub fn get(&self, key: &[u8], comparator: &dyn KeyComparator) -> Result<Option<Vec<u8>>> {
        // TODO: Implement hash-based lookup

        let mut iter = self.before(key, comparator)?;
        if let Some(kv) = iter.next() {
            let kv = kv?;

            if key.cmp(kv.key).is_eq() {}
        }

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
    */

    /// Creates an iterator that starts at the beginning of the block.
    pub fn iter(&'a self) -> DataBlockEntryIterator<'a> {
        DataBlockEntryIterator {
            remaining_entries: self.entries,
        }
    }

    /// Creates an iterator that begins with keys <= the given key where the
    /// first key seen is as close as possible to the given key.
    ///
    /// If the given key is not in the table, then the iterator may start after
    /// the given key.
    pub fn before(
        &'a self,
        key: &[u8],
        comparator: &dyn KeyComparator,
    ) -> Result<DataBlockKeyValueIterator<'a>> {
        // TODO: We need to test this logic. It could always return 0 and the remainder
        // of the code would still work, but that would be inefficient.
        let closest_offset = self.restart_search(key, self.restarts, comparator)?;

        let mut iter = DataBlockEntryIterator {
            remaining_entries: &self.entries[closest_offset..],
        }
        .rows();

        // Fast forward the iterator to the first value >= the seek key.
        // The maximum number of loop iterators is the restart_interval of the block.
        while let Some(peek) = iter.peek()? {
            // TODO: Performing the comparison here means that we will likely need to do
            // another comparison in the calling code to know if we seeked exactly to the
            // correct position. Consider moving the fast forward code up the call stack to
            // avoid
            match comparator.compare(key, peek.entry.key) {
                Ordering::Equal | Ordering::Less => break,
                Ordering::Greater => {
                    peek.consume();
                }
            }
        }

        Ok(iter)
    }

    /// NOTE: This assumes that restarts has a length of at least 1.
    // TODO: This will perform redundant entry parsing with the iterator.
    // ^ Possibly pre-parse all of the restart points?
    fn restart_search(
        &self,
        key: &[u8],
        restarts: &[u32],
        comparator: &dyn KeyComparator,
    ) -> Result<usize> {
        if restarts.len() == 1 {
            return Ok(restarts[0] as usize);
        }

        let mid_index = restarts.len() / 2;
        let mid_offset = restarts[mid_index] as usize;
        let (mid_entry, _) = DataBlockEntry::parse(&self.entries[mid_offset..])?;
        if mid_entry.shared_bytes != 0 {
            return Err(err_msg("Restart not valid"));
        }

        // TODO: Refactor to be non-recursive.
        match comparator.compare(key, mid_entry.key_delta) {
            Ordering::Equal => Ok(mid_offset as usize),
            Ordering::Less => self.restart_search(key, &restarts[..mid_index], comparator),
            Ordering::Greater => self.restart_search(key, &restarts[mid_index..], comparator),
        }
    }
}

#[derive(PartialEq)]
pub enum DataBlockIndexType {
    BinarySearch,
    BinaryAndHash,
}

#[derive(Debug)]
pub struct DataBlockEntry<'a> {
    /// Number of prefix bytes from the last entry's key which are the same as
    /// the key for the current entry.
    pub shared_bytes: u32,

    /// Additional unique key bytes for this entry coming after the shared ones.
    pub key_delta: &'a [u8],

    /// The complete value associated with this key.
    pub value: &'a [u8],
}

impl<'a> DataBlockEntry<'a> {
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
pub struct DataBlockEntryIterator<'a> {
    /// The remaining un-parsed entry data. This is a sub-slice of
    /// Block::entries.
    remaining_entries: &'a [u8],
}

impl<'a> DataBlockEntryIterator<'a> {
    pub fn rows(self) -> DataBlockKeyValueIterator<'a> {
        DataBlockKeyValueIterator {
            inner: self,
            last_key: vec![],
        }
    }

    pub fn is_empty(&self) -> bool {
        self.remaining_entries.is_empty()
    }

    pub fn peek(&self) -> Result<Option<(DataBlockEntry<'a>, &'a [u8])>> {
        if self.remaining_entries.len() == 0 {
            return Ok(None);
        }

        DataBlockEntry::parse(self.remaining_entries).map(|v| Some(v))
    }
}

impl<'a> Iterator for DataBlockEntryIterator<'a> {
    type Item = Result<DataBlockEntry<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining_entries.len() == 0 {
            return None;
        }

        Some(match DataBlockEntry::parse(self.remaining_entries) {
            Ok((entry, rest)) => {
                self.remaining_entries = rest;
                Ok(entry)
            }
            Err(e) => Err(e),
        })
    }
}

/*
The alternative is to use Bytes, but
*/

pub struct DataBlockKeyValueIterator<'a> {
    inner: DataBlockEntryIterator<'a>,
    last_key: Vec<u8>,
}

impl<'a> DataBlockKeyValueIterator<'a> {
    pub fn next<'b>(&'b mut self) -> Option<Result<KeyValuePair<'b, 'a>>> {
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

    // TODO: Deduplicate with next().
    pub fn peek<'b>(&'b mut self) -> Result<Option<DataBlockKeyValueIteratorPeek<'b, 'a>>> {
        match self.inner.peek()? {
            Some((entry, remaining_entries)) => {
                if entry.shared_bytes as usize > self.last_key.len() {
                    return Err(err_msg("Insufficient shared bytes from previous key."));
                }

                self.last_key.truncate(entry.shared_bytes as usize);
                self.last_key.extend_from_slice(entry.key_delta);

                Ok(Some(DataBlockKeyValueIteratorPeek {
                    entry: KeyValuePair {
                        key: &self.last_key,
                        value: entry.value,
                    },
                    remaining_entries,
                    remaining_entries_ptr: &mut self.inner.remaining_entries,
                }))
            }
            None => Ok(None),
        }
    }
}

// 'a is the lifetime of the iterator object
// 'b is the lifetime of the remaining_entries in the datablock
pub struct DataBlockKeyValueIteratorPeek<'a, 'b> {
    entry: KeyValuePair<'a, 'b>,
    remaining_entries: &'b [u8],
    remaining_entries_ptr: &'a mut &'b [u8],
}

impl<'a, 'b> DataBlockKeyValueIteratorPeek<'a, 'b> {
    fn consume(self) {
        *self.remaining_entries_ptr = self.remaining_entries;
    }
}

pub struct KeyValuePair<'a, 'b> {
    pub key: &'a [u8],
    pub value: &'b [u8],
}
