#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::intrinsics::unlikely;
use core::result::Result;

use common::list::{Appendable, ByteCounter};

use crate::types::FieldNumber;
use crate::{Enum, Message};

#[derive(Clone, Copy, Debug, Errable)]
#[cfg_attr(feature = "std", derive(Fail))]
#[repr(u32)]
pub enum WireError {
    InvalidWireType,
    UnexpectedWireType,
    Incomplete,
    InvalidString,
    IntegerOverflow,
    UnexpectedEndGroup,
}

impl core::fmt::Display for WireError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub type WireResult<T> = core::result::Result<T, WireError>;

pub fn serialize_varint<A: Appendable<Item = u8>>(mut v: u64, out: &mut A) -> Result<(), A::Error> {
    loop {
        let mut b = (v & 0x7f) as u8;
        v = v >> 7;
        if v != 0 {
            b |= 0x80;
            out.push(b)?;
        } else {
            out.push(b)?;
            break;
        }
    }

    Ok(())
}

pub fn parse_varint(input: &[u8]) -> WireResult<(u64, &[u8])> {
    let mut v = 0;
    let mut i = 0;

    // Maximum number of bytes to take.
    // Limited by size of input and size of 64bit integer.
    let max_bytes = core::cmp::min(input.len(), 10 /* ceil_div(64, 7) */);

    loop {
        let overflow = i >= max_bytes;
        if unsafe { unlikely(overflow) } {
            return Err(WireError::Incomplete);
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

fn decode_zigzag32(v: u64) -> WireResult<i32> {
    let n = v as i32;
    if (n as i64) != (v as i64) {
        return Err(WireError::IntegerOverflow);
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
    #[cfg(feature = "alloc")]
    StartGroup = 3,
    #[cfg(feature = "alloc")]
    EndGroup = 4,
    Word32 = 5,
}

impl WireType {
    fn from_usize(v: usize) -> WireResult<WireType> {
        Ok(match v {
            0 => WireType::Varint,
            1 => WireType::Word64,
            2 => WireType::LengthDelim,
            #[cfg(feature = "alloc")]
            3 => WireType::StartGroup,
            #[cfg(feature = "alloc")]
            4 => WireType::EndGroup,
            5 => WireType::Word32,
            _ => {
                return Err(WireError::InvalidWireType);
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
    fn parse(input: &[u8]) -> WireResult<(Tag, &[u8])> {
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
    fn serialize<A: Appendable<Item = u8>>(&self, out: &mut A) -> Result<(), A::Error> {
        let v = (self.field_number << 3) | (self.wire_type as u32);
        serialize_varint(v as u64, out)
    }
}

/// A single field in a message that was parsed from a binary stream.
///
/// This code is mainly used by the auto-generated code as follows:
/// - When serializing:
///   - call 'WireField::serialize_{type}()' for every present field value.
///   - if the field has no field presence (and isn't repeated),
///     `WireField::serialize_sparse_{type}()` should be called instead to avoid
///     appending fields with default values.
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

impl<'a> WireField<'a> {
    /// Parses all top level WireFields in the given data.
    /// TODO: Support parsing from Bytes?
    /// TODO: Make this return an iterator.
    #[cfg(feature = "alloc")]
    pub fn parse_all(input: &[u8]) -> WireResult<Vec<WireField>> {
        let mut out = vec![];

        for field in WireFieldIter::new(input) {
            out.push(field?);
        }

        Ok(out)
    }

    fn serialize<A: Appendable<Item = u8>>(&self, out: &mut A) -> Result<(), A::Error> {
        let wire_type = match self.value {
            WireValue::Varint(_) => WireType::Varint,
            WireValue::Word64(_) => WireType::Word64,
            WireValue::LengthDelim(_) => WireType::LengthDelim,
            WireValue::Word32(_) => WireType::Word32,
            #[cfg(feature = "alloc")]
            WireValue::Group(_) => WireType::StartGroup,
        };

        Tag {
            field_number: self.field_number,
            wire_type,
        }
        .serialize(out)?;

        self.value.serialize(out)?;

        #[cfg(feature = "alloc")]
        if wire_type == WireType::StartGroup {
            Tag {
                field_number: self.field_number,
                wire_type: WireType::EndGroup,
            }
            .serialize(out)?;
        }

        Ok(())
    }

    pub fn parse_double(&self) -> WireResult<f64> {
        Ok(f64::from_le_bytes(*self.value.word64()?))
    }

    pub fn serialize_double<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: f64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        let buf = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word64(&buf),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_double<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: f64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0.0 {
            Self::serialize_double(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_float(&self) -> WireResult<f32> {
        Ok(f32::from_le_bytes(*self.value.word32()?))
    }

    // pub fn parse_repeated_float

    pub fn serialize_float<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: f32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        let buf = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word32(&buf),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_float<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: f32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0.0 {
            Self::serialize_float(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_int32(&self) -> WireResult<i32> {
        Ok(self.value.varint()? as i32)
    }

    pub fn serialize_int32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::Varint(v as i64 as u64),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_int32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0 {
            Self::serialize_int32(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_int64(&self) -> WireResult<i64> {
        Ok(self.value.varint()? as i64)
    }

    pub fn serialize_int64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            // TODO: Probably need to extend first then serialize.
            value: WireValue::Varint(v as u64),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_int64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0 {
            Self::serialize_int64(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_uint32(&self) -> WireResult<u32> {
        Ok(self.value.varint()? as u32)
    }

    pub fn serialize_uint32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: u32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::Varint(v as u64),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_uint32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: u32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0 {
            Self::serialize_uint32(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_uint64(&self) -> WireResult<u64> {
        Ok(self.value.varint()? as u64)
    }

    pub fn serialize_uint64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: u64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::Varint(v),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_uint64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: u64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0 {
            Self::serialize_uint64(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_sint32(&self) -> WireResult<i32> {
        decode_zigzag32(self.value.varint()?)
    }

    pub fn serialize_sint32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::Varint(encode_zigzag32(v)),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_sint32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0 {
            Self::serialize_sint32(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_sint64(&self) -> WireResult<i64> {
        Ok(decode_zigzag64(self.value.varint()?))
    }

    pub fn serialize_sint64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::Varint(encode_zigzag64(v)),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_sint64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0 {
            Self::serialize_sint64(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_fixed32(&self) -> WireResult<u32> {
        Ok(u32::from_le_bytes(*self.value.word32()?))
    }

    pub fn serialize_fixed32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: u32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        let data = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word32(&data),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_fixed32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: u32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0 {
            Self::serialize_fixed32(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_fixed64(&self) -> WireResult<u64> {
        Ok(u64::from_le_bytes(*self.value.word64()?))
    }

    pub fn serialize_fixed64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: u64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        let data = v.to_le_bytes();
        WireField {
            field_number,
            value: WireValue::Word64(&data),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_fixed64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: u64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v != 0 {
            Self::serialize_fixed64(field_number, v, out)?
        }
        Ok(())
    }

    pub fn parse_sfixed32(&self) -> WireResult<i32> {
        Ok(i32::from_le_bytes(*self.value.word32()?))
    }

    pub fn serialize_sfixed32<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i32,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::Word32(&v.to_le_bytes()),
        }
        .serialize(out)
    }

    pub fn parse_sfixed64(&self) -> WireResult<i64> {
        Ok(i64::from_le_bytes(*self.value.word64()?))
    }

    pub fn serialize_sfixed64<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: i64,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::Word64(&v.to_le_bytes()),
        }
        .serialize(out)
    }

    pub fn parse_bool(&self) -> WireResult<bool> {
        Ok(self.value.varint()? != 0)
    }

    pub fn serialize_bool<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: bool,
        out: &mut A,
    ) -> Result<(), A::Error> {
        Self::serialize_uint32(field_number, if v { 1 } else { 0 }, out)
    }

    pub fn serialize_sparse_bool<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: bool,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v {
            Self::serialize_bool(field_number, v, out)?;
        }
        Ok(())
    }

    // no_std bytes
    // - Just an array
    // -

    pub fn parse_string<S: From<&'a str>>(&self) -> WireResult<S> {
        let bytes = self.value.length_delim()?;
        let s = core::str::from_utf8(bytes).map_err(|_| WireError::InvalidString)?;
        Ok(S::from(s))
    }

    pub fn serialize_string<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: &str,
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::LengthDelim(v.as_ref()),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_string<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: &str,
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v.len() > 0 {
            Self::serialize_string(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_bytes<B: From<&'a [u8]>>(&self) -> WireResult<B> {
        let bytes = self.value.length_delim()?;
        Ok(B::from(bytes))
    }

    pub fn serialize_bytes<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: &[u8],
        out: &mut A,
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::LengthDelim(v),
        }
        .serialize(out)
    }

    pub fn serialize_sparse_bytes<A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: &[u8],
        out: &mut A,
    ) -> Result<(), A::Error> {
        if v.len() > 0 {
            Self::serialize_bytes(field_number, v, out)?;
        }
        Ok(())
    }

    pub fn parse_enum<E: Enum>(&self) -> WireResult<E> {
        E::parse(self.parse_int32()?)
    }

    pub fn parse_enum_into(&self, out: &mut dyn Enum) -> WireResult<()> {
        out.assign(self.parse_int32()?)
    }

    pub fn serialize_enum<E: Enum, A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: &E,
        out: &mut A,
    ) -> Result<(), A::Error> {
        // TODO: Support up to 64bits?
        Self::serialize_int32(field_number, v.value(), out)
    }

    pub fn serialize_sparse_enum<E: Enum, A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        v: &E,
        out: &mut A,
    ) -> Result<(), A::Error> {
        // TODO: This one is tricky!
        if v.value() != 0 {
            Self::serialize_enum(field_number, v, out)?;
        }
        Ok(())
    }

    // TODO: Instead use a dynamic version that parses into an existing struct.
    pub fn parse_message<M: Message>(&self) -> WireResult<M> {
        let data = self.value.length_delim()?;
        M::parse(data)
    }

    pub fn parse_message_into<M: Message>(&self, message: &mut M) -> WireResult<()> {
        let data = self.value.length_delim()?;
        message.parse_merge(data)
    }

    pub fn serialize_sparse_message<
        M: Message + core::cmp::PartialEq + common::const_default::ConstDefault,
        A: Appendable<Item = u8>,
    >(
        field_number: FieldNumber,
        m: &M,
        out: &mut A,
    ) -> common::errors::Result<()> {
        if *m != M::DEFAULT {
            return Self::serialize_message(field_number, m, out);
        }

        Ok(())
    }

    #[cfg(feature = "alloc")]
    pub fn serialize_message<M: Message, A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        m: &M,
        out: &mut A,
    ) -> common::errors::Result<()> {
        let data = m.serialize()?;
        WireField {
            field_number,
            value: WireValue::LengthDelim(&data),
        }
        .serialize(out)?;
        Ok(())
    }

    /// When not having 'alloc', we first must fake serialize the message to
    /// figure out its serialized length and then serialize it for real after
    /// appending the tag and length bytes.
    ///
    /// TODO: Also make this the default mode once the length calculation
    /// becomes efficient for most message types.
    #[cfg(not(feature = "alloc"))]
    pub fn serialize_message<M: Message, A: Appendable<Item = u8>>(
        field_number: FieldNumber,
        m: &M,
        out: &mut A,
    ) -> common::errors::Result<()> {
        // TODO: optimize this when the size of messages is statically known (or for
        // repeated fields).
        let mut length_counter = ByteCounter::new();
        m.serialize_to(&mut length_counter)?;

        // TODO: Deduplicate this with the logic for serializing LengthDelim fields.
        Tag {
            field_number,
            wire_type: WireType::LengthDelim,
        }
        .serialize(out)?;
        serialize_varint(length_counter.total_bytes() as u64, out)?;
        m.serialize_to(out)?;

        Ok(())
    }
}

pub struct WireFieldIter<'a> {
    input: &'a [u8],

    #[cfg(feature = "alloc")]
    group: Option<Vec<WireValue<'a>>>,
}

impl<'a> WireFieldIter<'a> {
    pub fn new(input: &[u8]) -> WireFieldIter {
        WireFieldIter {
            input,
            #[cfg(feature = "alloc")]
            group: None,
        }
    }

    fn next_impl(&mut self) -> WireResult<Option<WireField<'a>>> {
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
                        return Err(WireError::Incomplete);
                    }
                    let v = array_ref![self.input, 0, 8];
                    self.input = &self.input[8..];
                    WireValue::Word64(v)
                }
                WireType::Word32 => {
                    if self.input.len() < 4 {
                        return Err(WireError::Incomplete);
                    }
                    let v = array_ref![self.input, 0, 4];
                    self.input = &self.input[4..];
                    WireValue::Word32(v)
                }
                WireType::LengthDelim => {
                    let (len, rest) = parse_varint(self.input)?;
                    let len = len as usize;
                    self.input = rest;
                    if self.input.len() < len {
                        return Err(WireError::Incomplete);
                    }
                    let v = &self.input[0..len];
                    self.input = &self.input[len..];
                    WireValue::LengthDelim(v)
                }
                #[cfg(feature = "alloc")]
                WireType::StartGroup => {
                    self.group = Some(vec![]);
                    continue;
                }
                #[cfg(feature = "alloc")]
                WireType::EndGroup => {
                    // TODO: Ensure that the start and end field numbers are
                    // consistent for groups.
                    let v = match self.group.take() {
                        Some(items) => WireValue::Group(items),
                        None => {
                            return Err(WireError::UnexpectedEndGroup);
                        }
                    };

                    v
                }
                _ => {
                    return Err(WireError::InvalidWireType);
                }
            };

            return Ok(Some(WireField {
                field_number: tag.field_number,
                value,
            }));
        }

        // If we reach this point, then the input is empty.

        #[cfg(feature = "alloc")]
        if self.group.is_some() {
            return Err(WireError::Incomplete);
        }

        Ok(None)
    }
}

impl<'a> core::iter::Iterator for WireFieldIter<'a> {
    type Item = WireResult<WireField<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_impl() {
            Ok(v) => v.map(|v| Ok(v)),
            Err(e) => Some(Err(e)),
        }
    }
}

// TODO: Deduplicate with the common crate implementation.
macro_rules! wire_enum_accessor {
    ($name:ident, $branch:ident, $t:ty) => {
        fn $name(&self) -> WireResult<$t> {
            if let Self::$branch(v) = self {
                Ok(*v)
            } else {
                Err(WireError::UnexpectedWireType)
            }
        }
    };
}

#[derive(Debug)]
pub enum WireValue<'a> {
    // TODO: Use u64 instead of usize
    Varint(u64),           // sint32, sint64, bool, enum
    Word64(&'a [u8; 8]),   // fixed64, sfixed64
    LengthDelim(&'a [u8]), // bytes, embedded messages, packed repeated fields
    #[cfg(feature = "alloc")]
    Group(Vec<WireValue<'a>>),
    Word32(&'a [u8; 4]),
}

impl<'a> WireValue<'a> {
    wire_enum_accessor!(varint, Varint, u64);
    wire_enum_accessor!(word64, Word64, &[u8; 8]);
    // wire_enum_accessor!(length_delim, LengthDelim, &[u8]);
    wire_enum_accessor!(word32, Word32, &[u8; 4]);

    fn length_delim(&self) -> WireResult<&'a [u8]> {
        if let Self::LengthDelim(v) = self {
            Ok(*v)
        } else {
            Err(WireError::UnexpectedWireType)
        }
    }

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
    fn repeated_word32(&self) -> WireResult<Vec<&[u8]>> {
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

    fn serialize<A: Appendable<Item = u8>>(&self, out: &mut A) -> Result<(), A::Error> {
        match self {
            WireValue::Varint(n) => serialize_varint(*n, out),
            WireValue::Word64(v) => out.extend_from_slice(&v[..]),
            WireValue::LengthDelim(v) => {
                serialize_varint(v.len() as u64, out)?;
                out.extend_from_slice(v)
            }
            #[cfg(feature = "alloc")]
            WireValue::Group(items) => {
                for i in items {
                    i.serialize(out)?;
                }

                Ok(())
            }
            WireValue::Word32(v) => out.extend_from_slice(&v[..]),
        }
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
