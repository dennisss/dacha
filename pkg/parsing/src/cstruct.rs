use common::errors::*;
use reflection::*;

use crate::binary::*;
use crate::take_exact;

// NOTE: The raw ones are unsafe since they are not portable due to lack of endian consistency
// and they don't validate that the struct is made of just primitive fields that can be trivially
// serialized without following memory pointers.
pub unsafe fn parse_cstruct_raw<T>(input: &[u8], out: &mut T) -> Option<usize> {
    let size = core::mem::size_of::<T>();
    if input.len() < size {
        return None;
    }

    let out_slice =
        unsafe { core::slice::from_raw_parts_mut(core::mem::transmute::<_, *mut u8>(out), size) };
    out_slice.copy_from_slice(&input[0..size]);

    Some(size)
}

pub unsafe fn serialize_cstruct_raw<'a, T>(input: &'a T) -> &'a [u8] {
    let size = core::mem::size_of::<T>();
    unsafe { core::slice::from_raw_parts(core::mem::transmute::<_, *const u8>(input), size) }
}


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
