use common::errors::*;
use protobuf::wire::parse_varint;
use crate::encoding::*;
use crate::internal_key::*;

// Types defined in https://github.com/facebook/rocksdb/blob/master/db/dbformat.h

// More internal key documentation:
// https://github.com/basho/leveldb/wiki/key-format

// Write batch format defined here:
// https://github.com/facebook/rocksdb/blob/2309fd63bf2c7fb1b45713b2bf4e879bdbdb4822/db/write_batch.cc


#[derive(Debug)]
pub struct WriteBatch<'a> {
	pub sequence: u64,
	pub writes: Vec<Write<'a>>
}

impl<'a> WriteBatch<'a> {
	pub fn parse(mut input: &'a [u8]) -> Result<(Self, &'a [u8])> {
		let sequence = parse_next!(input, parse_fixed64);
		let count = parse_next!(input, parse_fixed32);
		let mut writes = vec![];

		for _ in 0..count {
			let typ = ValueType::from_value(parse_next!(input, parse_u8))?;
			match typ {
				ValueType::Value => {
					let key = parse_next!(input, parse_slice);
					let value = parse_next!(input, parse_slice);
					writes.push(Write::Value { key, value });
				},
				ValueType::Deletion => {
					let key = parse_next!(input, parse_slice);
					writes.push(Write::Deletion { key });
				},
				_ => {
					return Err(
						format_err!("Unsupported value type: {:?}", typ));
				}
			}
		}

		Ok((Self { sequence, writes }, input))
	}
}

#[derive(Debug)]
pub enum Write<'a> {
	Value { key: &'a [u8], value: &'a [u8] },
	Deletion { key: &'a [u8] },
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