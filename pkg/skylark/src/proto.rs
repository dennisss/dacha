// Conversions to/from protobuf format.

use common::errors::*;
use protobuf::reflection::*;

use crate::dict::*;
use crate::primitives::*;
use crate::value::*;

pub fn value_to_proto(value: &dyn Value, mut proto: ReflectionMut) -> Result<()> {
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
        ReflectionMut::Repeated(_) => todo!(),
        ReflectionMut::Message(v) => {
            let dict = value
                .as_any()
                .downcast_ref::<DictValue>()
                .ok_or_else(|| err_msg("Expected dict value"))?;

            //
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
