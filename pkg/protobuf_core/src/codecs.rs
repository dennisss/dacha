use core::marker::PhantomData;

use common::list::{Appendable, ByteCounter};
use common::errors::Result;
use common::const_default::ConstDefault;

use crate::types::*;
use crate::wire::*;
use crate::{Enum, Message};

/*
/// A converter that translates some native Rust type to/from WireFields.
pub trait WireFieldCodec<'a> {
    type Type: 'a;

    /// Interprates a wire field's value as the the current codec's type. 
    ///
    /// NOTE: Parsing takes as input a WireValue and not a WireField as parsing should be
    /// independent on the field number.
    fn parse(value: &'a WireValue) -> WireResult<Self::Type>;

    /// Serializes a single value.
    /// This always pushes a single WireField into the output buffer.
    ///
    /// TODO: Evaluate if there are any performance concerns with this returning a WireError rather than A::Error which it is possible.
    fn serialize<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &Self::Type, out: &mut A
    ) -> WireResult<()>;

    /// Serializes a single value only if it is not equal to its default value.
    /// 
    /// TODO: This is not proto2 compatible as default values could be defined at the message level.
    fn serialize_sparse<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &Self::Type, out: &mut A
    ) -> WireResult<()>;
}
*/

macro_rules! define_varint_codec {
    ($name:ident, $t:ty, $from_wire:expr, $to_wire:expr, $default:expr) => {
		pub struct $name;

        impl $name {
            // TODO: Switch all of these back to taking a WireValue as input.
            pub fn parse(field: &WireField) -> WireResult<$t> {
                Ok(($from_wire)(field.value.varint()?))
            }

            pub fn serialize<A: Appendable<Item = u8>>(
                field_number: FieldNumber, value: $t, out: &mut A
            ) -> Result<(), A::Error> {
                WireField {
                    field_number,
                    value: WireValue::Varint(($to_wire)(value)),
                }
                .serialize(out)
            }

            pub fn serialize_sparse<A: Appendable<Item = u8>>(
                field_number: FieldNumber, value: $t, out: &mut A
            ) -> Result<(), A::Error> {
                if value != $default {
                    Self::serialize(field_number, value, out)?;
                }
                Ok(())
            }
        }
	};
}

macro_rules! define_word_codec {
    ( $name:ident, $t:ty, $variant:ident, $variant_parser:ident, $from_wire:expr, $to_wire:expr, $default:expr) => {
		pub struct $name;

        impl $name {
            pub fn parse(field: &WireField) -> WireResult<$t> {
                Ok(($from_wire)(*field.value.$variant_parser()?))
            }

            pub fn serialize<A: Appendable<Item = u8>>(
                field_number: FieldNumber, value: $t, out: &mut A
            ) -> Result<(), A::Error> {
                let buf = ($to_wire)(value);
                WireField {
                    field_number,
                    value: WireValue::$variant(&buf),
                }
                .serialize(out)
            }

            pub fn serialize_sparse<A: Appendable<Item = u8>>(
                field_number: FieldNumber, value: $t, out: &mut A
            ) -> Result<(), A::Error> {
                if value != $default {
                    Self::serialize(field_number, value, out)?;
                }
                Ok(())
            }
        }
	};
}


define_word_codec!(
    DoubleCodec,
    f64,
    Word64, word64,
    f64::from_le_bytes,
    f64::to_le_bytes,
    0.0
);

define_word_codec!(
    FloatCodec,
    f32,
    Word32, word32,
    f32::from_le_bytes,
    f32::to_le_bytes,
    0.0
);

define_varint_codec!(
    Int32Codec,
    i32,
    |v| v as i32,
    |v| v as i64 as u64,
    0
);

define_varint_codec!(
    Int64Codec,
    i64,
    |v| v as i64,
    // TODO: Test if we need to do sign extension
    |v| v as u64,
    0
);

define_varint_codec!(
    UInt32Codec,
    u32,
    |v| v as u32,
    |v| v as u64,
    0
);

define_varint_codec!(
    UInt64Codec,
    u64,
    |v| v as u64,
    |v| v as u64,
    0
);

pub struct SInt32Codec;

impl SInt32Codec {
    pub fn parse(field: &WireField) -> WireResult<i32> {
        decode_zigzag32(field.value.varint()?)
    }

    pub fn serialize<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: i32, out: &mut A
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::Varint(encode_zigzag32(value)),
        }
        .serialize(out)
    }

    pub fn serialize_sparse<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: i32, out: &mut A
    ) -> Result<(), A::Error> {
        if value != 0 {
            Self::serialize(field_number, value, out)?;
        }
        Ok(())
    }
}


define_varint_codec!(
    SInt64Codec,
    i64,
    decode_zigzag64,
    encode_zigzag64,
    0
);



define_word_codec!(
    Fixed32Codec,
    u32,
    Word32, word32,
    u32::from_le_bytes,
    u32::to_le_bytes,
    0
);

define_word_codec!(
    Fixed64Codec,
    u64,
    Word64, word64,
    u64::from_le_bytes,
    u64::to_le_bytes,
    0
);

define_word_codec!(
    SFixed32Codec,
    i32,
    Word32, word32,
    i32::from_le_bytes,
    i32::to_le_bytes,
    0
);

define_word_codec!(
    SFixed64Codec,
    i64,
    Word64, word64,
    i64::from_le_bytes,
    i64::to_le_bytes,
    0
);

define_varint_codec!(
    BoolCodec,
    bool,
    |v: u64| -> bool { v != 0 },
    |v: bool| -> u64 { if v { 1 } else { 0 } },
    false
);

pub struct StringCodec;

impl StringCodec {
    // TODO: Remove the From<> and just require the caller to convert it after?
    pub fn parse<'a, S: From<&'a str>>(field: &WireField<'a>) -> WireResult<S> {
        let bytes = field.value.length_delim()?;
        let s = core::str::from_utf8(bytes).map_err(|_| WireError::InvalidString)?;
        Ok(S::from(s))
    }

    pub fn serialize<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &str, out: &mut A
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::LengthDelim(value.as_bytes()),
        }
        .serialize(out)
    }

    pub fn serialize_sparse<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &str, out: &mut A
    ) -> Result<(), A::Error> {
        if value.len() > 0 {
            Self::serialize(field_number, value, out)?;
        }
        Ok(())
    }
}

pub struct BytesCodec;

impl BytesCodec {
    pub fn parse<'a, B>(field: &'a WireField) -> WireResult<B> where B: 'a + From<&'a [u8]> {
        let bytes = field.value.length_delim()?;
        Ok(B::from(bytes))
    }

    pub fn serialize<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &[u8], out: &mut A
    ) -> Result<(), A::Error> {
        WireField {
            field_number,
            value: WireValue::LengthDelim(value),
        }
        .serialize(out)
    }

    pub fn serialize_sparse<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &[u8], out: &mut A
    ) -> Result<(), A::Error> {
        if value.len() > 0 {
            Self::serialize(field_number, value, out)?;
        }
        Ok(())
    }
}


pub struct EnumCodec;

impl EnumCodec {
    pub fn parse<E: 'static + Enum>(field: &WireField) -> WireResult<E> {
        E::parse(Int32Codec::parse(field)?)
    }

    pub fn parse_into(field: &WireField, out: &mut dyn Enum) -> WireResult<()> {
        out.assign(Int32Codec::parse(field)?)?;
        Ok(())    
    }

    pub fn serialize<E: 'static + Enum, A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &E, out: &mut A
    ) -> Result<(), A::Error> {
        // TODO: Support up to 64bits?
        Int32Codec::serialize(field_number, value.value(), out)
    }

    pub fn serialize_sparse<E: 'static + Enum, A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &E, out: &mut A
    ) -> Result<(), A::Error> {
        // TODO: This one is tricky!
        if value.value() != 0 {
            Self::serialize(field_number, value, out)?;
        }
        Ok(())
    }


}

pub struct MessageCodec<M> {
    m: PhantomData<M>
}

impl<M: Message> MessageCodec<M> {
    pub fn parse(field: &WireField) -> WireResult<M> {
        // TODO: Instead use a dynamic version that parses into an existing struct.
        let data = field.value.length_delim()?;
        M::parse(data)
    }

    // TODO: Make sure all users of this clear the message first.
    pub fn parse_into(field: &WireField, message: &mut M) -> WireResult<()> {
        let data = field.value.length_delim()?;
        message.parse_merge(data)?;
        Ok(())
    }

    #[cfg(feature = "alloc")]
    pub fn serialize<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &M, out: &mut A
    ) -> Result<()> {
        let data = value.serialize()?;
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
    pub fn serialize<A: Appendable<Item = u8>>(
        field_number: FieldNumber, value: &M, out: &mut A
    ) -> Result<()> {
        // TODO: optimize this when the size of messages is statically known (or for
        // repeated fields).
        let mut length_counter = ByteCounter::new();
        value.serialize_to(&mut length_counter)?;

        // TODO: Deduplicate this with the logic for serializing LengthDelim fields.
        Tag {
            field_number,
            wire_type: WireType::LengthDelim,
        }
        .serialize(out)?;
        serialize_varint(length_counter.total_bytes() as u64, out)?;
        value.serialize_to(out)?;

        Ok(())
    }
}
