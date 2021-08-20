use common::errors::*;
use protobuf::wire::{parse_varint, serialize_varint};

#[derive(Debug)]
pub struct BlockHandle {
    pub offset: u64,
    pub size: u64,
}

impl BlockHandle {
    pub fn parse(input: &[u8]) -> Result<(Self, &[u8])> {
        let (offset, rest) = parse_varint(input)?;
        let (size, rest) = parse_varint(rest)?;
        Ok((Self { offset, size }, rest))
    }

    pub fn serialize(&self, output: &mut Vec<u8>) {
        serialize_varint(self.offset, output);
        serialize_varint(self.size, output);
    }

    /// TODO: Optimize this by returning a slice of a 16-byte buffer (as that's
    /// the biggest possible size of a serialized BlockHandle)
    pub fn serialized(&self) -> Vec<u8> {
        let mut out = vec![];
        self.serialize(&mut out);
        out
    }
}
