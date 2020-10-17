use super::{Enum, Message};
use byteorder::{ByteOrder, LittleEndian};
use bytes::{Bytes, BytesMut};
use common::errors::*;
use std::intrinsics::unlikely;

pub fn serialize_varint(mut v: usize, out: &mut Vec<u8>) {
    loop {
        let mut b = (v & 0x7f) as u8;
        v = v >> 7;
        if v != 0 {
            b &= 0x80;
            out.push(b);
        } else {
            out.push(b);
            break;
        }
    }
}

pub fn parse_varint(input: &[u8]) -> Result<(usize, &[u8])> {
    let mut v = 0;
    let mut i = 0;

    // Maximum number of bytes to take.
    // Limited by size of input and size of 64bit integer.
    let max_bytes = std::cmp::min(input.len(), 64 / 7);

    loop {
        let overflow = i >= max_bytes;
        if unsafe { unlikely(overflow) } {
            return Err(err_msg("To few/many bytes in varint"));
        }

        let mut b = input[i] as usize;
        let more = b & 0x80 != 0;
        b = b & 0x7f;

        v |= b << (7 * i);

        // Consume byte.
        i += 1;

        if !more {
            break;
        }
    }

    Ok((v, &input[i..]))
}

fn encode_zigzag32(n: usize) -> usize {
    (n << 1) ^ (n >> 31)
}
fn encode_zigzag64(n: usize) -> usize {
    (n << 1) ^ (n >> 63)
}

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
    field_number: usize,
    wire_type: WireType,
}

impl Tag {
    fn parse(input: &[u8]) -> Result<(Tag, &[u8])> {
        let (v, rest) = parse_varint(input)?;
        let wire_type = WireType::from_usize(v & 0b111)?;
        let field_number = v >> 3;
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
        let v = (self.field_number << 3) | (self.wire_type as usize);
        serialize_varint(v, out);
    }
}

/// A single field in a message that was parsed from a binary stream.
///
/// This code is mainly used by the auto-generated code as follows:
/// - When serializing:
///   - call 'WireField::serialize_{type}()' for every present field value.
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
    pub field_number: usize,

    // TODO: Make private
    pub value: WireValue<'a>,
}

impl WireField<'_> {
    /// Parses all top level WireFields in the given data.
    /// TODO: Support parsing from Bytes?
    /// TODO: Make this return an iterator.
    pub fn parse_all(mut input: &[u8]) -> Result<Vec<WireField>> {
        let mut out = vec![];
        let mut group = None;

        while input.len() > 0 {
            let (tag, rest) = Tag::parse(input)?;
            input = rest;
            let value = match tag.wire_type {
                WireType::Varint => {
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
                WireType::LengthDelim => {
                    let (len, rest) = parse_varint(input)?;
                    input = rest;
                    if input.len() < len {
                        return Err(err_msg("Too few bytes for length delimited"));
                    }
                    let v = &input[0..len];
                    input = &input[len..];
                    WireValue::LengthDelim(v)
                }
                WireType::StartGroup => {
                    group = Some(vec![]);
                    continue;
                }
                WireType::EndGroup => {
                    // TODO: Ensure that the start and end field numbers are
                    // consistent for groups.
                    let v = match group.take() {
                        Some(items) => WireValue::Group(items),
                        None => {
                            return Err(err_msg("Saw EndGroup before seeing a StartGroup"));
                        }
                    };

                    v
                }
                WireType::Word32 => {
                    if input.len() < 4 {
                        return Err(err_msg("Too few bytes for word32"));
                    }
                    let v = &input[0..4];
                    input = &input[4..];
                    WireValue::Word32(v)
                }
            };

            out.push(WireField {
                field_number: tag.field_number,
                value,
            });
        }

        if input.len() == 0 {
            if group.is_some() {
                return Err(err_msg("Unclosed group with no input remaining."));
            }

            Ok(out)
        } else {
            // This should pretty much never happen due to the while loop above
            Err(err_msg("Could not parse all input"))
        }
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

    pub fn serialize_double(field_number: usize, v: f64, out: &mut Vec<u8>) {
        let buf = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word64(&buf),
        }
        .serialize(out);
    }

    pub fn parse_float(&self) -> Result<f32> {
        Ok(LittleEndian::read_f32(self.value.word32()?))
    }

    pub fn serialize_float(field_number: usize, v: f32, out: &mut Vec<u8>) {
        let buf = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word32(&buf),
        }
        .serialize(out);
    }

    pub fn parse_int32(&self) -> Result<i32> {
        Ok(self.value.varint()? as i32)
    }

    pub fn serialize_int32(field_number: usize, v: i32, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(v as usize),
        }
        .serialize(out);
    }

    pub fn parse_int64(&self) -> Result<i64> {
        Ok(self.value.varint()? as i64)
    }

    pub fn serialize_int64(field_number: usize, v: i64, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(v as usize),
        }
        .serialize(out);
    }

    pub fn parse_uint32(&self) -> Result<u32> {
        Ok(self.value.varint()? as u32)
    }

    pub fn serialize_uint32(field_number: usize, v: u32, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(v as usize),
        }
        .serialize(out);
    }

    pub fn parse_uint64(&self) -> Result<u64> {
        Ok(self.value.varint()? as u64)
    }

    pub fn serialize_uint64(field_number: usize, v: u64, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::Varint(v as usize),
        }
        .serialize(out);
    }

    // parse_sint32
    // parse_sint64

    pub fn parse_fixed32(&self) -> Result<u32> {
        Ok(LittleEndian::read_u32(self.value.word32()?))
    }

    pub fn serialize_fixed32(field_number: usize, v: u32, out: &mut Vec<u8>) {
        let data = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word32(&data),
        }
        .serialize(out);
    }

    pub fn parse_fixed64(&self) -> Result<u64> {
        Ok(LittleEndian::read_u64(self.value.word64()?))
    }

    pub fn serialize_fixed64(field_number: usize, v: u64, out: &mut Vec<u8>) {
        let data = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word64(&data),
        }
        .serialize(out);
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

    pub fn serialize_bool(field_number: usize, v: bool, out: &mut Vec<u8>) {
        Self::serialize_uint32(field_number, if v { 1 } else { 0 }, out);
    }

    pub fn parse_string(&self) -> Result<String> {
        let mut val = vec![];
        val.extend_from_slice(self.value.length_delim()?);
        String::from_utf8(val).map_err(|_| err_msg("Invalid utf-8 bytes in string"))
    }

    pub fn serialize_string(field_number: usize, v: &str, out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::LengthDelim(v.as_ref()),
        }
        .serialize(out);
    }

    pub fn parse_bytes(&self) -> Result<BytesMut> {
        let mut val = vec![];
        val.extend_from_slice(self.value.length_delim()?);
        Ok(BytesMut::from(val))
    }

    pub fn serialize_bytes(field_number: usize, v: &[u8], out: &mut Vec<u8>) {
        WireField {
            field_number,
            value: WireValue::LengthDelim(v),
        }
        .serialize(out);
    }

    pub fn parse_enum<E: Enum>(&self) -> Result<E> {
        E::from_usize(self.value.varint()?)
    }

    pub fn serialize_enum<E: Enum>(field_number: usize, v: E, out: &mut Vec<u8>) {
        // TODO: Support up to 64bits?
        Self::serialize_uint32(field_number, v.to_usize() as u32, out);
    }

    pub fn parse_message<M: Message>(&self) -> Result<M> {
        let data = self.value.length_delim()?;
        M::parse(Bytes::from(data))
    }

    pub fn serialize_message<M: Message>(
        field_number: usize,
        m: M,
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

#[derive(Debug)]
pub enum WireValue<'a> {
    // TODO: Use u64 instead of usize
    Varint(usize),         // sint32, sint64, bool, enum
    Word64(&'a [u8]),      // fixed64, sfixed64
    LengthDelim(&'a [u8]), // bytes, embedded messages, packed repeated fields
    Group(Vec<WireValue<'a>>),
    Word32(&'a [u8]),
}

// TODO: Move to common library.
macro_rules! enum_accessor {
    ($name:ident, $branch:ident, $t:ty) => {
        fn $name(&self) -> Result<$t> {
            if let Self::$branch(v) = self {
                Ok(*v)
            } else {
                Err(err_msg("Unexpected value type."))
            }
        }
    };
}

impl WireValue<'_> {
    enum_accessor!(varint, Varint, usize);
    enum_accessor!(word64, Word64, &[u8]);
    enum_accessor!(length_delim, LengthDelim, &[u8]);
    enum_accessor!(word32, Word32, &[u8]);

    fn serialize(&self, out: &mut Vec<u8>) {
        match self {
            WireValue::Varint(n) => serialize_varint(*n, out),
            WireValue::Word64(v) => out.extend_from_slice(&v),
            WireValue::LengthDelim(v) => {
                serialize_varint(v.len(), out);
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
