use common::errors::*;
use protobuf::wire::parse_varint;

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

pub fn serialize_write_batch(sequence: u64, writes: &[Write], out: &mut Vec<u8>) {
    out.extend_from_slice(&sequence.to_le_bytes());
    out.extend_from_slice(&(writes.len() as u32).to_le_bytes());

    for write in writes {
        match write {
            Write::Value { key, value } => {
                out.push(ValueType::Value.to_value());
                serialize_slice(*key, out);
                serialize_slice(*value, out);
            }
            Write::Deletion { key } => {
                out.push(ValueType::Deletion.to_value());
                serialize_slice(*key, out);
            }
        }
    }
}

// TODO: Ensure that all changes in a WriteBatch touch distinct keys. Otherwise
// we can't apply all of the writes with the same sequence.
pub struct WriteBatch {
    data: Vec<u8>,
}

impl WriteBatch {
    pub fn new() -> Self {
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

    fn increment_count(&mut self) {
        let count_ref = array_mut_ref![self.data, 8, 4];
        let mut count = u32::from_le_bytes(*count_ref);
        count += 1;
        *count_ref = count.to_le_bytes();
    }

    pub fn put(&mut self, key: &[u8], value: &[u8]) -> &mut Self {
        self.increment_count();

        self.data.push(ValueType::Value.to_value());
        serialize_slice(key, &mut self.data);
        serialize_slice(value, &mut self.data);

        self
    }

    pub fn delete(&mut self, key: &[u8]) -> &mut Self {
        self.increment_count();

        self.data.push(ValueType::Deletion.to_value());
        serialize_slice(key, &mut self.data);

        self
    }

    pub fn clear(&mut self) {
        self.data.truncate(0);
        self.data.resize(8 + 4, 0);
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

// WriteBatch::rep_ :=
//    sequence: fixed64
//    count: fixed32
//    data: record[count]
// record :=
//    kTypeValue varstring varstring
//    kTypeDeletion varstring
//    kTypeSingleDeletion varstring
//    kTypeRangeDeletion varstring varstring
//    kTypeMerge varstring varstring
//    kTypeColumnFamilyValue varint32 varstring varstring
//    kTypeColumnFamilyDeletion varint32 varstring
//    kTypeColumnFamilySingleDeletion varint32 varstring
//    kTypeColumnFamilyRangeDeletion varint32 varstring varstring
//    kTypeColumnFamilyMerge varint32 varstring varstring
//    kTypeBeginPrepareXID varstring
//    kTypeEndPrepareXID
//    kTypeCommitXID varstring
//    kTypeRollbackXID varstring
//    kTypeBeginPersistedPrepareXID varstring
//    kTypeBeginUnprepareXID varstring
//    kTypeNoop
// varstring :=
//    len: varint32
//    data: uint8[len]
