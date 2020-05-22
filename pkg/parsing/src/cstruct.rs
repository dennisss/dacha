use crate::binary::*;
use crate::take_exact;
use common::errors::*;
use reflection::*;

// TODO: Verify that if we ever deserialize using serde that we check for
// trailing blocks.

pub fn parse_cstruct_le<'a>(mut input: &'a [u8], output: &mut dyn Reflect) -> Result<&'a [u8]> {
    for field_idx in 0..output.fields_len() {
        let field = output.fields_index_mut(field_idx);
        input = match field.value {
            ReflectValue::U64(v) => {
                let (num, rest) = le_u64(input)?;
                *v = num;
                rest
            }
            ReflectValue::I64(v) => {
                let (num, rest) = le_i64(input)?;
                *v = num;
                rest
            }
            ReflectValue::U32(v) => {
                let (num, rest) = le_u32(input)?;
                *v = num;
                rest
            }
            ReflectValue::I32(v) => {
                let (num, rest) = le_i32(input)?;
                *v = num;
                rest
            }
            ReflectValue::U16(v) => {
                let (num, rest) = le_u16(input)?;
                *v = num;
                rest
            }
            ReflectValue::U8(v) => {
                let (num, rest) = be_u8(input)?;
                *v = num;
                rest
            }
            ReflectValue::U8Slice(v) => {
                let (data, rest) = take_exact(v.len())(input)?;
                v.copy_from_slice(data);
                rest
            }
            _ => {
                return Err(err_msg("Unsupported C-Struct type"));
            }
        };
    }

    Ok(input)
}

pub fn parse_cstruct_be<'a>(mut input: &'a [u8], output: &mut dyn Reflect) -> Result<&'a [u8]> {
    for field_idx in 0..output.fields_len() {
        let field = output.fields_index_mut(field_idx);
        input = match field.value {
            ReflectValue::U64(v) => {
                let (num, rest) = be_u64(input)?;
                *v = num;
                rest
            }
            ReflectValue::I64(v) => {
                let (num, rest) = be_i64(input)?;
                *v = num;
                rest
            }
            ReflectValue::U32(v) => {
                let (num, rest) = be_u32(input)?;
                *v = num;
                rest
            }
            ReflectValue::I32(v) => {
                let (num, rest) = be_i32(input)?;
                *v = num;
                rest
            }
            ReflectValue::U16(v) => {
                let (num, rest) = be_u16(input)?;
                *v = num;
                rest
            }
            ReflectValue::I16(v) => {
                let (num, rest) = be_i16(input)?;
                *v = num;
                rest
            }
            ReflectValue::U8(v) => {
                let (num, rest) = be_u8(input)?;
                *v = num;
                rest
            }
            ReflectValue::U8Slice(v) => {
                let (data, rest) = take_exact(v.len())(input)?;
                v.copy_from_slice(data);
                rest
            }
            _ => {
                return Err(err_msg("Unsupported C-Struct type"));
            }
        };
    }

    Ok(input)
}
