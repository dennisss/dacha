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

    BadDescriptor,

    /// While interprating a well formed wire value as an enum, we couldn't find
    /// any known enum variant with the given integer value.
    UnknownEnumVariant,
}

impl core::fmt::Display for WireError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub type WireResult<T> = core::result::Result<T, WireError>;

pub fn serialize_varint<A: Appendable<Item = u8> + ?Sized>(
    mut v: u64,
    out: &mut A,
) -> Result<(), A::Error> {
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

fn parse_word32<'a>(input: &'a [u8]) -> WireResult<(&'a [u8; 4], &'a [u8])> {
    if input.len() < 4 {
        return Err(WireError::Incomplete);
    }
    let v = array_ref![input, 0, 4];
    let rest = &input[4..];
    Ok((v, rest))
}

fn parse_word64<'a>(input: &'a [u8]) -> WireResult<(&'a [u8; 8], &'a [u8])> {
    if input.len() < 8 {
        return Err(WireError::Incomplete);
    }
    let v = array_ref![input, 0, 8];
    let rest = &input[8..];
    Ok((v, rest))
}

pub fn encode_zigzag32(n: i32) -> u64 {
    ((n << 1) ^ (n >> 31)) as i64 as u64
}

pub fn decode_zigzag32(v: u64) -> WireResult<i32> {
    let n = v as i32;
    if (n as i64) != (v as i64) {
        return Err(WireError::IntegerOverflow);
    }
    // TODO: Verify that we didn't lose precision (by casting back)

    Ok((n >> 1) ^ (-(n & 1)))
}

pub fn encode_zigzag64(n: i64) -> u64 {
    ((n << 1) ^ (n >> 63)) as u64
}

pub fn decode_zigzag64(v: u64) -> i64 {
    let n = v as i64;
    (n >> 1) ^ (-(n & 1))
}

// k = (n << 1) ^ (n >> 31)

#[derive(PartialEq, Clone, Copy)]
pub enum WireType {
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

/// TODO: Rename WireTag.
pub(crate) struct Tag {
    // TODO: Figure out exactly what type this is allowed to be.
    pub field_number: FieldNumber,
    pub wire_type: WireType,
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
    pub fn serialize<A: Appendable<Item = u8> + ?Sized>(
        &self,
        out: &mut A,
    ) -> Result<(), A::Error> {
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

    /// PREFER to use the codecs over using this function
    /// NOTE: Compiled code shouldn't use this.
    pub fn serialize<A: Appendable<Item = u8> + ?Sized>(
        &self,
        out: &mut A,
    ) -> Result<(), A::Error> {
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
                    let (v, rest) = parse_word64(&self.input)?;
                    self.input = rest;
                    WireValue::Word64(v)
                }
                WireType::Word32 => {
                    let (v, rest) = parse_word32(&self.input)?;
                    self.input = rest;
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
        pub fn $name(&self) -> WireResult<$t> {
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

    pub fn length_delim(&self) -> WireResult<&'a [u8]> {
        if let Self::LengthDelim(v) = self {
            Ok(*v)
        } else {
            Err(WireError::UnexpectedWireType)
        }
    }

    /// Interprets this value as containing zero or more varints.
    ///
    /// TODO: To support backwards compatibility of making a field repeated,
    /// should we also read singular fields this way and drop all but the first
    /// value (using an iterator interface)?
    pub fn repeated_varint(&self) -> impl Iterator<Item = WireResult<u64>> + 'a {
        WireValuePackedIterator {
            parser: parse_varint,
            state: match self {
                Self::Varint(v) => WireValuePackedIteratorState::Singular(*v),
                Self::LengthDelim(input) => WireValuePackedIteratorState::LengthDelim(input),
                _ => WireValuePackedIteratorState::Error(WireError::UnexpectedWireType),
            },
        }
    }

    pub fn repeated_word32(&self) -> impl Iterator<Item = WireResult<&'a [u8; 4]>> {
        WireValuePackedIterator {
            parser: parse_word32,
            state: match self {
                Self::Word32(v) => WireValuePackedIteratorState::Singular(*v),
                Self::LengthDelim(input) => WireValuePackedIteratorState::LengthDelim(input),
                _ => WireValuePackedIteratorState::Error(WireError::UnexpectedWireType),
            },
        }
    }

    pub fn repeated_word64(&self) -> impl Iterator<Item = WireResult<&'a [u8; 8]>> {
        WireValuePackedIterator {
            parser: parse_word64,
            state: match self {
                Self::Word64(v) => WireValuePackedIteratorState::Singular(*v),
                Self::LengthDelim(input) => WireValuePackedIteratorState::LengthDelim(input),
                _ => WireValuePackedIteratorState::Error(WireError::UnexpectedWireType),
            },
        }
    }

    pub fn serialize<A: Appendable<Item = u8> + ?Sized>(
        &self,
        out: &mut A,
    ) -> Result<(), A::Error> {
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

struct WireValuePackedIterator<'a, T, F> {
    state: WireValuePackedIteratorState<'a, T>,
    parser: F,
}

enum WireValuePackedIteratorState<'a, T> {
    Singular(T),
    LengthDelim(&'a [u8]),
    Error(WireError),
}

impl<'a, T: Copy, F> Iterator for WireValuePackedIterator<'a, T, F>
where
    F: Fn(&'a [u8]) -> WireResult<(T, &'a [u8])>,
{
    type Item = WireResult<T>;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.state {
            WireValuePackedIteratorState::Singular(v) => {
                let out = *v;
                self.state = WireValuePackedIteratorState::LengthDelim(&[]);
                Some(Ok(out))
            }
            WireValuePackedIteratorState::LengthDelim(ref mut input) => {
                if input.is_empty() {
                    return None;
                }

                match (self.parser)(input) {
                    Ok((v, rest)) => {
                        *input = rest;
                        Some(Ok(v))
                    }
                    Err(e) => Some(Err(e)),
                }
            }
            WireValuePackedIteratorState::Error(e) => Some(Err(*e)),
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

        for value in (0..10_000).chain(VALUES.iter().cloned()) {
            let mut out = vec![];
            serialize_varint(value, &mut out);
            let (val, rest) = parse_varint(&out).unwrap();
            assert_eq!(val, value);
            assert_eq!(rest.len(), 0);
        }

        let mut overflow_data = [0xffu8; 11];
        // overflow_data[9] = 0x7f;

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
