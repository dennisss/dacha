use std::ops::Deref;

use common::errors::*;
use protobuf::wire::{parse_varint, serialize_varint};

use crate::db::internal_key::*;
use crate::encoding::*;
use crate::memtable::memtable::MemTable;
use crate::record_log::RecordReader;

// Types defined in https://github.com/facebook/rocksdb/blob/master/db/dbformat.h

// More internal key documentation:
// https://github.com/basho/leveldb/wiki/key-format

// Write batch format defined here:
// https://github.com/facebook/rocksdb/blob/2309fd63bf2c7fb1b45713b2bf4e879bdbdb4822/db/write_batch.cc

pub struct WriteBatchIterator<'a> {
    input: &'a [u8],
    sequence: u64,
    remaining_count: u32,
}

impl<'a> WriteBatchIterator<'a> {
    pub fn new(mut input: &'a [u8]) -> Result<Self> {
        let sequence = parse_next!(input, parse_fixed64);
        let count = parse_next!(input, parse_fixed32);

        Ok(Self {
            input,
            sequence,
            remaining_count: count,
        })
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    fn next_impl(&mut self) -> Result<Option<Write<'a>>> {
        if self.remaining_count == 0 {
            return Ok(None);
        }

        self.remaining_count -= 1;

        let typ = ValueType::from_value(parse_next!(self.input, parse_u8))?;
        Ok(Some(match typ {
            ValueType::Value => {
                let key = parse_next!(self.input, parse_slice);
                let value = parse_next!(self.input, parse_slice);
                Write::Value { key, value }
            }
            ValueType::Deletion => {
                let key = parse_next!(self.input, parse_slice);
                Write::Deletion { key }
            }
            _ => {
                return Err(format_err!("Unsupported value type: {:?}", typ));
            }
        }))
    }

    pub fn remaining_input(self) -> &'a [u8] {
        self.input
    }

    pub async fn apply(&mut self, table: &MemTable) -> Result<()> {
        while let Some(w) = self.next() {
            let w = w?;
            match w {
                Write::Value { key, value } => {
                    let ikey = InternalKey {
                        user_key: key,
                        typ: ValueType::Value,
                        sequence: self.sequence(),
                    }
                    .serialized();

                    table.insert(ikey, value.to_vec()).await;
                }
                Write::Deletion { key } => {
                    let ikey = InternalKey {
                        user_key: key,
                        typ: ValueType::Deletion,
                        sequence: self.sequence(),
                    }
                    .serialized();

                    table.insert(ikey, vec![]).await;
                }
            }
        }

        if self.input.len() != 0 {
            return Err(err_msg("Extra data after write batch"));
        }

        Ok(())
    }

    /// Writes WriteBatches from the given log file and applies their effects
    /// to the current table.
    pub async fn read_table(
        log: &mut RecordReader,
        table: &MemTable,
        last_sequence: &mut u64,
    ) -> Result<()> {
        // TODO: Ignore duplicate keys.

        while let Some(record) = log.read().await? {
            let mut batch = WriteBatchIterator::new(&record)?;
            batch.apply(table).await?;
            *last_sequence = std::cmp::max(*last_sequence, batch.sequence());
        }

        Ok(())
    }
}

impl<'a> Iterator for WriteBatchIterator<'a> {
    type Item = Result<Write<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_impl() {
            Ok(Some(v)) => Some(Ok(v)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

#[derive(Debug)]
pub enum Write<'a> {
    Value { key: &'a [u8], value: &'a [u8] },
    Deletion { key: &'a [u8] },
}

/// Batch of writes to execute on the database.
///
/// Note that we require all keys touched in a batch to be unique and inserted
/// into the batch in lexicographically sorted order. Uniqueness is required for
/// correctness since we can't have two of the same key for the same sequence.
///
/// TODO: May need to adjust the sorting if we ever support different
/// comparators.
#[derive(Clone)]
pub struct WriteBatch {
    data: Vec<u8>,
}

impl WriteBatch {
    /// NOTE: This is only useful inside of the WriteBatchBuilder.
    fn new() -> Self {
        let data = vec![0u8; 8 + 4];
        Self { data }
    }

    /// NOTE: This will not have a meaningful value until after the batch has
    /// been written.
    pub fn sequence(&self) -> u64 {
        u64::from_le_bytes(*array_ref![self.data, 0, 8])
    }

    /// Set a custom sequence value for this batch. This sequence must be
    /// greater than all previous sequences seen by the database.
    ///
    /// NOTE: Specifying a custom sequence for the batch is an advanced feature
    /// and should generally not be used. When not specified a new unique
    /// sequence is automatically generated.
    ///
    /// TODO: Check that the sequence fits within 56 bits.
    pub fn set_sequence(&mut self, sequence: u64) {
        self.data[0..8].copy_from_slice(&sequence.to_le_bytes());
    }

    pub fn count(&self) -> usize {
        let count_ref = array_ref![self.data, 8, 4];
        u32::from_le_bytes(*count_ref) as usize
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        // TODO: Perform way more validation
        Ok(Self {
            data: data.to_vec(),
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    pub fn iter(&self) -> Result<WriteBatchIterator> {
        WriteBatchIterator::new(&self.data)
    }
}

/// Builds a WriteBatch. The same restrictions about key ordering apply when
/// using this.
pub struct WriteBatchBuilder {
    batch: WriteBatch,

    /// Offset and length of the last written key within 'data'.
    last_key: Option<(usize, usize)>,
}

impl WriteBatchBuilder {
    pub fn new() -> Self {
        Self {
            batch: WriteBatch::new(),
            last_key: None,
        }
    }

    fn increment_count(&mut self) {
        let count_ref = array_mut_ref![self.batch.data, 8, 4];
        let mut count = u32::from_le_bytes(*count_ref);
        count += 1;
        *count_ref = count.to_le_bytes();
    }

    fn push_key(&mut self, key: &[u8]) {
        if let Some((last_key_index, last_key_len)) = self.last_key.take() {
            let last_key = &self.batch.data[last_key_index..(last_key_index + last_key_len)];
            assert!(last_key < key);
        }

        serialize_varint(key.len() as u64, &mut self.batch.data);

        let i = self.batch.data.len();
        self.batch.data.extend_from_slice(key);
        self.last_key = Some((i, key.len()));
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> &mut Self {
        self.increment_count();
        self.batch.data.push(ValueType::Value.to_value());
        self.push_key(key);
        serialize_slice(value, &mut self.batch.data);
        self
    }

    pub fn delete(&mut self, key: &[u8]) -> &mut Self {
        self.increment_count();
        self.batch.data.push(ValueType::Deletion.to_value());
        self.push_key(key);
        self
    }

    pub fn clear(&mut self) {
        self.batch.data.truncate(0);
        self.batch.data.resize(8 + 4, 0);
        self.last_key = None;
    }

    pub fn build(self) -> WriteBatch {
        self.batch
    }
}

impl Deref for WriteBatchBuilder {
    type Target = WriteBatch;

    fn deref(&self) -> &Self::Target {
        &self.batch
    }
}
