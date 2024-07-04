use std::marker::PhantomData;
use std::mem::discriminant;

use base_error::*;
use common::const_default::ConstDefault;
use datastore_meta_client::key_encoding::KeyEncoder;
use parsing::parse_next;
use protobuf::reflection::{Reflect, Reflection, ReflectionMut};
use protobuf::MessageReflection;

use crate::db::table::*;

pub struct KeyBuilder<Tag: ProtobufTableTag> {
    out: Vec<u8>,
    key_index: usize,
    next_field_index: usize,
    data: PhantomData<Tag>,
}

// TODO: Remove templating from class and put more responsibility on the user.
impl<Tag: ProtobufTableTag> KeyBuilder<Tag> {
    pub fn message_key(key_index: usize, message: &Tag::Message) -> Result<Vec<u8>> {
        let mut builder = Self::new(key_index);

        // TODO: Add a bounds check here (or just have one in Self::new()).
        let key_config = &Tag::indexed_keys()[key_index];

        for field in key_config.fields {
            let r = message
                .field_by_number(field.number.raw())
                .ok_or_else(|| err_msg("Missing index field value"))?;

            // NOTE: Since we are iterating over the fields from the config and reflecting a
            // Tag::Message type, the types and field order are correct.
            builder.append_raw(field, r)?;
        }

        Ok(builder.finish())
    }

    pub fn new(key_index: usize) -> Self {
        let mut out = vec![];
        KeyEncoder::encode_varuint(Tag::table_id() as u64, false, &mut out);
        KeyEncoder::encode_varuint(key_index as u64, false, &mut out);

        Self {
            out,
            key_index,
            next_field_index: 0,
            data: PhantomData,
        }
    }

    pub fn append(&mut self, value: Reflection) -> Result<()> {
        // TODO: Need more bounds checking.

        let field = &Tag::indexed_keys()[self.key_index].fields[self.next_field_index];
        self.next_field_index += 1;

        if discriminant(&value)
            != discriminant(
                &Tag::Message::DEFAULT
                    .field_by_number(field.number.raw())
                    .unwrap(),
            )
        {
            todo!()
        }

        self.append_raw(field, value)
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.out
    }

    /// Appends one field to the key.
    ///
    /// This assumes:
    /// - Fields are appended in the right order.
    /// - The value is the correct data type.
    fn append_raw(
        &mut self,
        field: &ProtobufKeyField<Tag::Message>,
        value: Reflection,
    ) -> Result<()> {
        let inverted = field.direction == Direction::Descending;

        match value {
            // TODO: Use encode_end_bytes if it is the last field.
            Reflection::String(v) => {
                KeyEncoder::encode_bytes(v.as_bytes(), &mut self.out);
            }
            Reflection::Bytes(v) => {
                KeyEncoder::encode_bytes(v, &mut self.out);
            }
            // TODO: Detect stuff like fixed32 and appropriately use fixed encoding here too.
            Reflection::U32(v) => {
                if field.fixed_size {
                    KeyEncoder::encode_u32(*v, inverted, &mut self.out)
                } else {
                    KeyEncoder::encode_varuint(*v as u64, inverted, &mut self.out)
                }
            }
            Reflection::U64(v) => {
                if field.fixed_size {
                    KeyEncoder::encode_u64(*v, inverted, &mut self.out);
                } else {
                    KeyEncoder::encode_varuint(*v, inverted, &mut self.out)
                }
            }
            // Reflection::I32(v) => ,
            // Reflection::I64(_) => todo!(),
            // Reflection::Bool(_) => todo!(),
            _ => {
                return Err(err_msg("Index contains un-indexable field"));
            }
        }

        Ok(())
    }

    pub fn decode_key(
        key_config: &ProtobufTableKey<Tag::Message>,
        key_index: usize,
        mut key: &[u8],
        message: &mut Tag::Message,
    ) -> Result<()> {
        let actual_table_id = parse_next!(key, |input| KeyEncoder::decode_varuint(input, false));
        if actual_table_id != Tag::table_id() as u64 {
            return Err(err_msg("Wrong table id"));
        }

        let actual_key_index = parse_next!(key, |input| KeyEncoder::decode_varuint(input, false));
        if actual_key_index != key_index as u64 {
            return Err(err_msg("Wrong key index"));
        }

        for field in key_config.fields {
            let r = message
                .field_by_number_mut(field.number.raw())
                .ok_or_else(|| err_msg("Missing index field value"))?;

            let inverted = field.direction == Direction::Descending;

            match r {
                ReflectionMut::String(v) => {
                    let bytes = parse_next!(key, KeyEncoder::decode_bytes);
                    *v = String::from_utf8(bytes)?;
                }
                ReflectionMut::Bytes(v) => {
                    // *v = parse
                    todo!()
                }
                ReflectionMut::U32(v) => {
                    if field.fixed_size {
                        *v = parse_next!(key, |input| KeyEncoder::decode_u32(input, inverted));
                    } else {
                        *v = parse_next!(key, |input| KeyEncoder::decode_varuint(input, inverted))
                            as u32;
                    }
                }
                ReflectionMut::U64(v) => {
                    if field.fixed_size {
                        *v = parse_next!(key, |input| KeyEncoder::decode_u64(input, inverted));
                    } else {
                        *v = parse_next!(key, |input| KeyEncoder::decode_varuint(input, inverted));
                    }
                }
                _ => {
                    return Err(err_msg("Index contains un-indexable field"));
                }
            }
        }

        if !key.is_empty() {
            return Err(err_msg("Could not parse entire row key"));
        }

        Ok(())
    }
}
