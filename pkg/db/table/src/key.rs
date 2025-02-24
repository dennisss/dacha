use std::marker::PhantomData;
use std::mem::discriminant;

use base_error::*;
use parsing::parse_next;
use protobuf::reflection::{Reflect, Reflection, ReflectionMut};
use protobuf::MessageReflection;

use crate::key_encoding::KeyEncoder;
use crate::reflection::{field_by_path, field_by_path_mut};
use crate::table::*;

/// Constructs the keys used as keys of rows in the key value store (where each
/// key has a subset of the fields in a message as defined by the corresponding
/// index definition).
///
/// NOTE: The encoding of byte and string fields is fully compatible and bytes
/// can be compared to strings and vise versa (this is necessary since we do all
/// key range calculations in byte form for both data types).
pub struct KeyBuilder<'a> {
    out: Vec<u8>,
    index_key_config: &'a ProtobufTableKey,
    next_field_index: usize,
    default_message: &'a dyn MessageReflection,
}

impl<'a> KeyBuilder<'a> {
    pub fn message_key(
        table_id: u32,
        index_key_config: &'a ProtobufTableKey,
        message: &'a dyn MessageReflection,
    ) -> Result<Vec<u8>> {
        let mut builder = Self::new(table_id, index_key_config, message);

        for field in index_key_config.fields {
            let r = field_by_path(message, field.path)?;

            // NOTE: Since we are iterating over the fields from the config and reflecting a
            // Tag::Message type, the types and field order are correct.
            builder.append_raw(field, r)?;
        }

        Ok(builder.finish())
    }

    pub fn new(
        table_id: u32,
        index_key_config: &'a ProtobufTableKey,
        default_message: &'a dyn MessageReflection,
    ) -> Self {
        let mut out = vec![];
        KeyEncoder::encode_varuint(table_id as u64, false, &mut out);
        KeyEncoder::encode_varuint(index_key_config.index_id as u64, false, &mut out);

        Self {
            out,
            index_key_config,
            next_field_index: 0,
            default_message,
        }
    }

    pub fn append(&mut self, mut value: Reflection) -> Result<()> {
        // TODO: Maybe move to append_raw
        if let Reflection::String(s) = value {
            value = Reflection::Bytes(s.as_bytes());
        }

        let field = &self.index_key_config.fields[self.next_field_index];
        self.next_field_index += 1;

        let mut default_value = field_by_path(self.default_message, field.path).unwrap();
        if let Reflection::String(s) = default_value {
            default_value = Reflection::Bytes(s.as_bytes());
        }

        if discriminant(&value) != discriminant(&default_value) {
            return Err(err_msg("Incompatible values being packed into key."));
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
    fn append_raw(&mut self, field: &ProtobufKeyField, value: Reflection) -> Result<()> {
        let inverted = field.direction == Direction::Descending;

        // NOTE: We don't use 'encode_end_bytes' for the final field to eventually
        // support adding column family indexes after the keys.
        match value {
            Reflection::String(v) => {
                if inverted {
                    return Err(err_msg("Inverted string keys not supported"));
                }

                KeyEncoder::encode_bytes(v.as_bytes(), &mut self.out);
            }
            Reflection::Bytes(v) => {
                if inverted {
                    return Err(err_msg("Inverted bytes keys not supported"));
                }

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
        table_id: u32,
        index_key_config: &ProtobufTableKey,
        mut key: &[u8],
        message: &mut dyn MessageReflection,
    ) -> Result<()> {
        let actual_table_id = parse_next!(key, |input| KeyEncoder::decode_varuint(input, false));
        if actual_table_id != table_id as u64 {
            return Err(err_msg("Wrong table id"));
        }

        let actual_key_index = parse_next!(key, |input| KeyEncoder::decode_varuint(input, false));
        if actual_key_index != index_key_config.index_id as u64 {
            return Err(err_msg("Wrong key index"));
        }

        for field in index_key_config.fields {
            let r = field_by_path_mut(message, field.path)?;

            let inverted = field.direction == Direction::Descending;

            match r {
                ReflectionMut::String(v) => {
                    let bytes = parse_next!(key, KeyEncoder::decode_bytes);
                    *v = String::from_utf8(bytes)?;
                }
                ReflectionMut::Bytes(v) => {
                    let bytes = parse_next!(key, KeyEncoder::decode_bytes);
                    v.clear();
                    v.extend_from_slice(&bytes);
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
