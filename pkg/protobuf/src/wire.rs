use std::intrinsics::unlikely;

use common::bytes::{Bytes, BytesMut};
use common::errors::*;
use protobuf_compiler::spec::FieldNumber;
use byteorder::{ByteOrder, LittleEndian};

use crate::{Enum, Message, BytesField};


pub fn serialize_varint(mut v: u64, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v = v >> 7;
        if v != 0 {
            b |= 0x80;
            out.push(b);
        } else {
            out.push(b);
            break;
        }
    }
}

pub fn parse_varint(input: &[u8]) -> Result<(u64, &[u8])> {
    let mut v = 0;
    let mut i = 0;

    // Maximum number of bytes to take.
    // Limited by size of input and size of 64bit integer.
    let max_bytes = std::cmp::min(input.len(), 10 /* ceil_div(64, 7) */);

    loop {
        let overflow = i >= max_bytes;
        if unsafe { unlikely(overflow) } {
            return Err(err_msg("To few/many bytes in varint"));
        }

        let mut b = input[i] as u64;
        let more = b & 0x80 != 0;
        b = b & 0x7f;

        // TODO: Should we care if the 10th byte has truncated bits?
        v |= b << (7 * i);

        // Consume byte.
        i += 1;

        if !more {
            break;
        }
    }

    Ok((v, &input[i..]))
}

fn encode_zigzag32(n: i32) -> u64 {
    ((n << 1) ^ (n >> 31)) as i64 as u64
}

fn decode_zigzag32(v: u64) -> Result<i32> {
    let n = v as i32;
    if (n as i64) != (v as i64) {
        return Err(err_msg("Lost precision when casting to i32"));
    }
    // TODO: Verify that we didn't lose precision (by casting back)

    Ok((n >> 1) ^ (-(n & 1)))
}

fn encode_zigzag64(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

fn decode_zigzag64(v: u64) -> i64 {
    let n = v as i64;
    (n >> 1) ^ (-(n & 1))
}

// k = (n << 1) ^ (n >> 31)

#[derive(PartialEq, Clone, Copy)]
enum WireType {
    Varint = 0,
    Word64 = 1,
    LengthDelim = 2,
    StartGroup = 3,
    EndGroup = 4,
    Word32 = 5,
}

impl WireType {
    fn from_usize(v: usize) -> Result<WireType> {
        Ok(match v {
            0 => WireType::Varint,
            1 => WireType::Word64,
            2 => WireType::LengthDelim,
            3 => WireType::StartGroup,
            4 => WireType::EndGroup,
            5 => WireType::Word32,
            _ => {
                return Err(err_msg("Invalid wire type number"));
            }
        })
    }
}

struct Tag {
    // TODO: Figure out exactly what type this is allowed to be.
    field_number: FieldNumber,
    wire_type: WireType,
}

impl Tag {
    fn parse(input: &[u8]) -> Result<(Tag, &[u8])> {
        let (v, rest) = parse_varint(input)?;
        let wire_type = WireType::from_usize((v as usize) & 0b111)?;
        let field_number = (v >> 3) as u32;
        Ok((
            Tag {
                field_number,
                wire_type,
            },
            rest,
        ))
    }

    // TODO: Ensure field_number is within the usize range
    fn serialize(&self, out: &mut Vec<u8>) {
        let v = (self.field_number << 3) | (self.wire_type as u32);
        serialize_varint(v as u64, out);
    }
}

/// A single field in a message that was parsed from a binary stream.
///
/// This code is mainly used by the auto-generated code as follows:
/// - When serializing:
///   - call 'WireField::serialize_{type}()' for every present field value.
///   - if the field has no field presence (and isn't repeated),
///     `WireField::serialize_sparse_{type}()` should be called instead to
///     avoid appending fields with default values.
/// - When parsing:
///   - call 'WireField::parse_all()' to get all fields in an input stream.
///   - then call field.parse_{type}() to downcast to the expected type.
///
/// TODO: Consider moving the parsing functions to the WireValue struct as they
/// have nothing to do with the field number
/// ^ Currently they are in the same struct to ensure that the code for the
/// parse and serialize paths are right next to each other to ensure consistent
/// wire types are used.
#[derive(Debug)]
pub struct WireField<'a> {
    pub field_number: FieldNumber,

    // TODO: Make private
    pub value: WireValue<'a>,
}

impl WireField<'_> {
    /// Parses all top level WireFields in the given data.
    /// TODO: Support parsing from Bytes?
    /// TODO: Make this return an iterator.
    pub fn parse_all(input: &[u8]) -> Result<Vec<WireField>> {
        let mut out = vec![];
        
        for field in WireFieldIter::new(input) {
            out.push(field?);
        }

        Ok(out)
    }

    fn serialize(&self, out: &mut Vec<u8>) {
        let wire_type = match self.value {
            WireValue::Varint(_) => WireType::Varint,
            WireValue::Word64(_) => WireType::Word64,
            WireValue::LengthDelim(_) => WireType::LengthDelim,
            WireValue::Word32(_) => WireType::Word32,
            WireValue::Group(_) => WireType::StartGroup,
        };

        Tag {
            field_number: self.field_number,
            wire_type,
        }
        .serialize(out);

        self.value.serialize(out);

        if wire_type == WireType::StartGroup {
            Tag {
                field_number: self.field_number,
                wire_type: WireType::EndGroup,
            }
            .serialize(out);
        }
    }

    pub fn parse_double(&self) -> Result<f64> {
        Ok(LittleEndian::read_f64(self.value.word64()?))
    }

    pub fn serialize_double(field_number: FieldNumber, v: f64, out: &mut Vec<u8>) {
        let buf = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word64(&buf),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_double(field_number: FieldNumber, v: f64, out: &mut Vec<u8>) {
        if v != 0.0 {
            Self::serialize_double(field_number, v, out);
        }
    }

    pub fn parse_float(&self) -> Result<f32> {
        Ok(LittleEndian::read_f32(self.value.word32()?))
    }

    // pub fn parse_repeated_float

    pub fn serialize_float(field_number: FieldNumber, v: f32, out: &mut Vec<u8>) {
        let buf = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word32(&buf),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_float(field_number: FieldNumber, v: f32, out: &mut Vec<u8>) {
        if v != 0.0 {
            Self::serialize_float(field_number, v, out);
        }
    }

    pub fn parse_int32(&self) -> Result<i32> {
        Ok(self.value.varint()? as i32)
    }

    pub fn serialize_int32(field_number: FieldNumber, v: i32, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(v as i64 as u64),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_int32(field_number: FieldNumber, v: i32, out: &mut Vec<u8>) {
        if v != 0 {
            Self::serialize_int32(field_number, v, out);
        }
    }

    pub fn parse_int64(&self) -> Result<i64> {
        Ok(self.value.varint()? as i64)
    }

    pub fn serialize_int64(field_number: FieldNumber, v: i64, out: &mut Vec<u8>) {
        WireField {
            field_number,
            // TODO: Probably need to extend first then serialize.
            value: WireValue::Varint(v as u64),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_int64(field_number: FieldNumber, v: i64, out: &mut Vec<u8>) {
        if v != 0 {
            Self::serialize_int64(field_number, v, out);
        }
    }

    pub fn parse_uint32(&self) -> Result<u32> {
        Ok(self.value.varint()? as u32)
    }

    pub fn serialize_uint32(field_number: FieldNumber, v: u32, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(v as u64),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_uint32(field_number: FieldNumber, v: u32, out: &mut Vec<u8>) {
        if v != 0 {
            Self::serialize_uint32(field_number, v, out);
        }
    }

    pub fn parse_uint64(&self) -> Result<u64> {
        Ok(self.value.varint()? as u64)
    }

    pub fn serialize_uint64(field_number: FieldNumber, v: u64, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(v),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_uint64(field_number: FieldNumber, v: u64, out: &mut Vec<u8>) {
        if v != 0 {
            Self::serialize_uint64(field_number, v, out);
        }
    }

    pub fn parse_sint32(&self) -> Result<i32> {
        decode_zigzag32(self.value.varint()?)
    }

    pub fn serialize_sint32(field_number: FieldNumber, v: i32, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(encode_zigzag32(v)),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_sint32(field_number: FieldNumber, v: i32, out: &mut Vec<u8>) {
        if v != 0 {
            Self::serialize_sint32(field_number, v, out);
        }
    }

    pub fn parse_sint64(&self) -> Result<i64> {
        Ok(decode_zigzag64(self.value.varint()?))
    }

    pub fn serialize_sint64(field_number: FieldNumber, v: i64, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(encode_zigzag64(v)),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_sint64(field_number: FieldNumber, v: i64, out: &mut Vec<u8>) {
        if v != 0 {
            Self::serialize_sint64(field_number, v, out);
        }
    }

    pub fn parse_fixed32(&self) -> Result<u32> {
        Ok(LittleEndian::read_u32(self.value.word32()?))
    }

    pub fn serialize_fixed32(field_number: FieldNumber, v: u32, out: &mut Vec<u8>) {
        let data = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word32(&data),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_fixed32(field_number: FieldNumber, v: u32, out: &mut Vec<u8>) {
        if v != 0 {
            Self::serialize_fixed32(field_number, v, out);
        }
    }

    pub fn parse_fixed64(&self) -> Result<u64> {
        Ok(LittleEndian::read_u64(self.value.word64()?))
    }

    pub fn serialize_fixed64(field_number: FieldNumber, v: u64, out: &mut Vec<u8>) {
        let data = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word64(&data),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_fixed64(field_number: FieldNumber, v: u64, out: &mut Vec<u8>) {
        if v != 0 {
            Self::serialize_fixed64(field_number, v, out);
        }
    }

    pub fn parse_sfixed32(&self) -> Result<i32> {
        Ok(LittleEndian::read_i32(self.value.word32()?))
    }
    pub fn parse_sfixed64(&self) -> Result<i64> {
        Ok(LittleEndian::read_i64(self.value.word64()?))
    }

    pub fn parse_bool(&self) -> Result<bool> {
        Ok(self.value.varint()? != 0)
    }

    pub fn serialize_bool(field_number: FieldNumber, v: bool, out: &mut Vec<u8>) {
        Self::serialize_uint32(field_number, if v { 1 } else { 0 }, out);
    }

    pub fn serialize_sparse_bool(field_number: FieldNumber, v: bool, out: &mut Vec<u8>) {
        if v {
            Self::serialize_bool(field_number, v, out);
        }
    }

    pub fn parse_string(&self) -> Result<String> {
        let mut val = vec![];
        val.extend_from_slice(self.value.length_delim()?);
        String::from_utf8(val).map_err(|_| err_msg("Invalid utf-8 bytes in string"))
    }

    pub fn serialize_string(field_number: FieldNumber, v: &str, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::LengthDelim(v.as_ref()),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_string(field_number: FieldNumber, v: &str, out: &mut Vec<u8>) {
        if v.len() > 0 {
            Self::serialize_string(field_number, v, out);
        } 
    }

    pub fn parse_bytes(&self) -> Result<BytesField> {
        let mut val = vec![];
        val.extend_from_slice(self.value.length_delim()?);
        Ok(BytesField::from(val))
    }

    pub fn serialize_bytes(field_number: FieldNumber, v: &[u8], out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::LengthDelim(v),
        }
        .serialize(out);
    }

    pub fn serialize_sparse_bytes(field_number: FieldNumber, v: &[u8], out: &mut Vec<u8>) {
        if v.len() > 0 {
            Self::serialize_bytes(field_number, v, out);
        }
    }

    pub fn parse_enum<E: Enum>(&self) -> Result<E> {
        E::parse(self.parse_int32()?)
    }

    pub fn serialize_enum<E: Enum>(field_number: FieldNumber, v: &E, out: &mut Vec<u8>) {
        // TODO: Support up to 64bits?
        Self::serialize_int32(field_number, v.value(), out);
    }

    pub fn serialize_sparse_enum<E: Enum>(field_number: FieldNumber, v: &E, out: &mut Vec<u8>) {
        // TODO: This one is tricky!
        if v.value() != 0 {
            Self::serialize_enum(field_number, v, out);
        }
    }

    // TODO: Instead use a dynamic version that parses into an existing struct.
    pub fn parse_message<M: Message>(&self) -> Result<M> {
        let data = self.value.length_delim()?;
        M::parse(data)
    }

    pub fn serialize_sparse_message<M: Message + std::cmp::PartialEq + common::const_default::ConstDefault>(
        field_number: FieldNumber,
        m: &M,
        out: &mut Vec<u8>
    ) -> Result<()> {
        if *m != M::DEFAULT {
            return Self::serialize_message(field_number, m, out);
        }

        Ok(())
    }

    pub fn serialize_message(
        field_number: FieldNumber,
        m: &dyn Message,
        out: &mut Vec<u8>,
    ) -> Result<()> {
        let data = m.serialize()?;
        WireField {
            field_number,
            value: WireValue::LengthDelim(&data),
        }
        .serialize(out);
        Ok(())
    }
}

pub struct WireFieldIter<'a> {
    input: &'a [u8],
    group: Option<Vec<WireValue<'a>>>
}

impl<'a> WireFieldIter<'a> {
    pub fn new(input: &[u8]) -> WireFieldIter {
        WireFieldIter {
            input,
            group: None
        }
    }

    fn next_impl(&mut self) -> Result<Option<WireField<'a>>> {
        while !self.input.is_empty() {
            let (tag, rest) = Tag::parse(self.input)?;
            self.input = rest;
            let value = match tag.wire_type {
                WireType::Varint => {
                    // TODO: In some cases, 

                    let (v, rest) = parse_varint(self.input)?;
                    self.input = rest;
                    WireValue::Varint(v)
                }
                WireType::Word64 => {
                    if self.input.len() < 8 {
                        return Err(err_msg("Too few bytes for word64"));
                    }
                    let v = &self.input[0..8];
                    self.input = &self.input[8..];
                    WireValue::Word64(v)
                }
                WireType::Word32 => {
                    if self.input.len() < 4 {
                        return Err(err_msg("Too few bytes for word32"));
                    }
                    let v = &self.input[0..4];
                    self.input = &self.input[4..];
                    WireValue::Word32(v)
                }
                WireType::LengthDelim => {
                    let (len, rest) = parse_varint(self.input)?;
                    let len = len as usize;
                    self.input = rest;
                    if self.input.len() < len {
                        return Err(err_msg("Too few bytes for length delimited"));
                    }
                    let v = &self.input[0..len];
                    self.input = &self.input[len..];
                    WireValue::LengthDelim(v)
                }
                WireType::StartGroup => {
                    self.group = Some(vec![]);
                    continue;
                }
                WireType::EndGroup => {
                    // TODO: Ensure that the start and end field numbers are
                    // consistent for groups.
                    let v = match self.group.take() {
                        Some(items) => WireValue::Group(items),
                        None => {
                            return Err(err_msg("Saw EndGroup before seeing a StartGroup"));
                        }
                    };

                    v
                }
            };

            return Ok(Some(WireField {
                field_number: tag.field_number,
                value,
            }));
        }

        // If we reach this point, then the input is empty.

        if self.group.is_some() {
            return Err(err_msg("Unclosed group with no input remaining."));
        }

        Ok(None)
    }
}

impl<'a> std::iter::Iterator for WireFieldIter<'a> {
    type Item = Result<WireField<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_impl() {
            Ok(v) => v.map(|v| Ok(v)),
            Err(e) => Some(Err(e))
        }
    }
}

#[derive(Debug)]
pub enum WireValue<'a> {
    // TODO: Use u64 instead of usize
    Varint(u64),           // sint32, sint64, bool, enum
    Word64(&'a [u8]),      // fixed64, sfixed64
    LengthDelim(&'a [u8]), // bytes, embedded messages, packed repeated fields
    Group(Vec<WireValue<'a>>),
    Word32(&'a [u8]),
}

impl WireValue<'_> {
    enum_accessor!(varint, Varint, u64);
    enum_accessor!(word64, Word64, &[u8]);
    enum_accessor!(length_delim, LengthDelim, &[u8]);
    enum_accessor!(word32, Word32, &[u8]);

    /*
    WireType::Varint => {
        // TODO: In some cases, 

        let (v, rest) = parse_varint(input)?;
        input = rest;
        WireValue::Varint(v)
    }
    WireType::Word64 => {
        if input.len() < 8 {
            return Err(err_msg("Too few bytes for word64"));
        }
        let v = &input[0..8];
        input = &input[8..];
        WireValue::Word64(v)
    }
    WireType::Word32 => {
        if input.len() < 4 {
            return Err(err_msg("Too few bytes for word32"));
        }
        
        WireValue::Word32(v)
    }
    */

    /*
    fn repeated_word32(&self) -> Result<Vec<&[u8]>> {
        let mut out = vec![]
        match self {
            Self::Word32(v) => { out.push(v) },
            Self::LengthDelim(mut input) => {
                while input.len() >= 4 {
                    let v = &input[0..4];
                    input = &input[4..];
                }

                if input.len() != 0 {
                    return Err(err_msg("Packed word32 field contains too many/few bytes"));
                }
            }
        }

        Ok(out)
    }
    */

    // Now we do the same thing for varint and 


    fn serialize(&self, out: &mut Vec<u8>) {
        match self {
            WireValue::Varint(n) => serialize_varint(*n, out),
            WireValue::Word64(v) => out.extend_from_slice(&v),
            WireValue::LengthDelim(v) => {
                serialize_varint(v.len() as u64, out);
                out.extend_from_slice(v);
            }
            WireValue::Group(items) => {
                for i in items {
                    i.serialize(out);
                }
            }
            WireValue::Word32(v) => out.extend_from_slice(v),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint() {
        // TODO: Need to also test partial parsing if there is more data after the
        // varint.

        const VALUES: &[u64] = &[100000, std::u64::MAX];

        for value in VALUES {
            let mut out = vec![];
            serialize_varint(*value, &mut out);
            let (val, rest) = parse_varint(&out).unwrap();
            assert_eq!(val, *value);
            assert_eq!(rest.len(), 0);
        }

        let mut overflow_data = [0xffu8; 10];
        overflow_data[9] = 0x7f;

        let res = parse_varint(&overflow_data);
        println!("{:x?}", res);
        assert!(res.is_err());
    }

    #[test]
    fn test_zigzag() {
        // TODO:
        // const VALUES: &[]
    }
}
