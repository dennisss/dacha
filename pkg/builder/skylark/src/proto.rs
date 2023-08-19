// Conversions to/from protobuf format.

use common::errors::*;
use protobuf::reflection::*;

use crate::dict::*;
use crate::list::*;
use crate::primitives::*;
use crate::value::*;

/// NOTE: This function will internally ensure that 'value' is allocated to a
/// frame.
pub fn value_to_proto(
    value: &dyn Value,
    mut proto: ReflectionMut,
    frame: &mut ValueCallFrame,
) -> Result<()> {
    // Mainly to block infinite recursion.
    let mut frame = frame.child(value)?;

    match proto {
        ReflectionMut::F32(v) => {
            *v = value
                .downcast_float()
                .ok_or_else(|| err_msg("Expected float value"))? as f32
        }
        ReflectionMut::F64(v) => {
            *v = value
                .downcast_float()
                .ok_or_else(|| err_msg("Expected float value"))?
        }
        ReflectionMut::I32(v) => {
            *v = value
                .downcast_int()
                .ok_or_else(|| err_msg("Expected int value"))? as i32
        }
        ReflectionMut::I64(v) => {
            *v = value
                .downcast_int()
                .ok_or_else(|| err_msg("Expected int value"))?
        }
        ReflectionMut::U32(v) => {
            *v = value
                .downcast_int()
                .ok_or_else(|| err_msg("Expected int value"))? as u32
        }
        ReflectionMut::U64(v) => {
            *v = value
                .downcast_int()
                .ok_or_else(|| err_msg("Expected int value"))? as u64
        }
        ReflectionMut::Bool(v) => {
            *v = value
                .downcast_bool()
                .ok_or_else(|| err_msg("Expected bool value"))?
        }
        ReflectionMut::String(v) => {
            *v = value
                .downcast_string()
                .ok_or_else(|| err_msg("Expected string value"))?
                .into()
        }
        ReflectionMut::Bytes(_) => todo!(),
        ReflectionMut::Repeated(v) => {
            let list = value
                .as_any()
                .downcast_ref::<ListValue>()
                .ok_or_else(|| err_msg("Expected list value"))?;

            for value in list.iter() {
                let value = value.upgrade_or_error()?;

                value_to_proto(&*value, v.reflect_add(), &mut frame)?;
            }
        }
        ReflectionMut::Message(v) => {
            let dict = value
                .as_any()
                .downcast_ref::<DictValue>()
                .ok_or_else(|| err_msg("Expected dict value"))?;

            for (key, value) in dict.iter() {
                let key = key.upgrade_or_error()?;
                let value = value.upgrade_or_error()?;

                let key_str = key
                    .downcast_string()
                    .ok_or_else(|| err_msg("Expected string value"))?;

                let field_num = v
                    .field_number_by_name(key_str)
                    .ok_or_else(|| format_err!("Unknown field named: {}", key_str))?;

                let field = v.field_by_number_mut(field_num).unwrap();
                value_to_proto(&*value, field, &mut frame)?;
            }
        }
        ReflectionMut::Enum(v) => {
            let name = value
                .downcast_string()
                .ok_or_else(|| err_msg("Expected string value"))?;

            v.assign_name(name)?;
        }
        ReflectionMut::Set(_) => todo!(),
    }

    Ok(())
}
