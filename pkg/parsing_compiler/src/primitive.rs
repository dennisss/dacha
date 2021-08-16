use common::errors::*;

use crate::proto::dsl::PrimitiveType;

pub struct PrimitiveTypeImpl {}

impl PrimitiveTypeImpl {
    pub fn typename(typ: PrimitiveType) -> Result<&'static str> {
        Ok(match typ {
            PrimitiveType::UNKNOWN => {
                return Err(err_msg("No primitive type specified"));
            }
            PrimitiveType::U8 => "u8",
            PrimitiveType::I8 => "i8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::I16 => "i16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::I32 => "i32",
            PrimitiveType::U64 => "u64",
            PrimitiveType::I64 => "i64",
            PrimitiveType::FLOAT => "f32",
            PrimitiveType::DOUBLE => "f64",
            PrimitiveType::BOOL => "bool",
        })
    }

    pub fn sizeof(typ: PrimitiveType) -> Result<usize> {
        Ok(match typ {
            PrimitiveType::UNKNOWN => {
                return Err(err_msg("No primitive type specified"));
            }
            PrimitiveType::U8 => 1,
            PrimitiveType::I8 => 1,
            PrimitiveType::U16 => 2,
            PrimitiveType::I16 => 2,
            PrimitiveType::U32 => 4,
            PrimitiveType::I32 => 4,
            PrimitiveType::U64 => 8,
            PrimitiveType::I64 => 8,
            PrimitiveType::FLOAT => 4,
            PrimitiveType::DOUBLE => 8,
            PrimitiveType::BOOL => 1, // TODO: Check this?
        })
    }
}
