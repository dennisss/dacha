use common::errors::*;
use common::line_builder::*;

use crate::proto::*;
use crate::size::SizeExpression;
use crate::types::*;

/// TODO: Include the bit width in the type description directly?
pub struct PrimitiveType {
    proto: PrimitiveTypeProto,
    endian: Endian,
}

impl PrimitiveType {
    pub fn create(proto: PrimitiveTypeProto, endian: Endian) -> Self {
        Self { proto, endian }
    }

    fn endian_str(&self) -> Result<&'static str> {
        Ok(match self.endian {
            Endian::LITTLE_ENDIAN => "le",
            Endian::BIG_ENDIAN => "be",
            Endian::UNKNOWN => {
                return Err(err_msg("Unspecified endian"));
            }
        })
    }

    fn typename(&self) -> Result<&'static str> {
        Ok(match self.proto {
            PrimitiveTypeProto::UNKNOWN => {
                return Err(err_msg("No primitive type specified"));
            }
            PrimitiveTypeProto::U8 => "u8",
            PrimitiveTypeProto::I8 => "i8",
            PrimitiveTypeProto::U16 => "u16",
            PrimitiveTypeProto::I16 => "i16",
            PrimitiveTypeProto::U32 => "u32",
            PrimitiveTypeProto::I32 => "i32",
            PrimitiveTypeProto::U64 => "u64",
            PrimitiveTypeProto::I64 => "i64",
            PrimitiveTypeProto::FLOAT => "f32",
            PrimitiveTypeProto::DOUBLE => "f64",
            PrimitiveTypeProto::BOOL => "bool",
        })
    }
}

impl Type for PrimitiveType {
    fn type_expression(&self) -> Result<String> {
        Ok(self.typename()?.to_string())
    }

    fn value_expression(&self, value: &Value) -> Result<String> {
        if value.int64_value().len() != 1 {
            return Err(err_msg("Unsupported value"));
        }

        Ok(format!("{}", value.int64_value()[0]))
    }

    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String> {
        Ok(format!(
            "parse_next!(input, ::parsing::binary::{}_{})",
            self.endian_str()?,
            self.typename()?
        ))
    }

    fn parse_bits_expression(&self, bit_offset: usize, bit_width: usize) -> Result<String> {
        if self.endian != Endian::BIG_ENDIAN {
            return Err(err_msg("Bit fields only supported in big endian mode"));
        }

        let desc = BitField::new(bit_offset, bit_width, self.proto)?;

        let mut lines = LineBuilder::new();
        lines.add("{");
        lines.add(format!("let mut buf = [0u8; {}];", desc.int_size));

        // Copy the bytes into the buffer with it aligned as far to the right of the
        // buffer as possible.
        lines.add(format!(
            "(&mut buf[{}..{}]).copy_from_slice(&bit_input[{}..{}]);",
            desc.int_size - (desc.end_byte - desc.start_byte),
            desc.int_size,
            desc.start_byte,
            desc.end_byte
        ));

        lines.add(format!(
            "let mut val = {}::from_be_bytes(buf);",
            desc.int_type
        ));

        lines.add(format!("val = (val >> {}) & {};", desc.shift, desc.mask));

        // Now we'll cast it to the final type
        if self.proto == PrimitiveTypeProto::BOOL {
            lines.add("val != 0");
        } else {
            lines.add(format!("val as {}", self.type_expression()?));
        }

        lines.add("}");

        Ok(lines.to_string())
    }

    fn serialize_bytes_expression(
        &self,
        value: &str,
        context: &TypeParserContext,
    ) -> Result<String> {
        Ok(format!(
            "out.extend_from_slice(&{}.to_{}_bytes());",
            value,
            self.endian_str()?
        ))
    }

    fn serialize_bits_expression(
        &self,
        value: &str,
        bit_offset: usize,
        bit_width: usize,
    ) -> Result<String> {
        if self.endian != Endian::BIG_ENDIAN {
            return Err(err_msg("Bit fields only supported in big endian mode"));
        }

        let desc = BitField::new(bit_offset, bit_width, self.proto)?;

        let mut lines = LineBuilder::new();
        lines.add("{");

        lines.add(format!("let raw_v = {};", value));

        // validate that the value in memory can fit into the serialized number of bits
        // without truncation.
        if self.proto != PrimitiveTypeProto::BOOL {
            lines.add(format!("if raw_v & {} != raw_v {{", desc.mask));
            lines.add("return Err(err_msg(\"Value too large for bit field\"));");
            lines.add("}");
        }

        // Put value into known intermediate integer format.
        lines.add(format!("let mut v = raw_v as {};", desc.int_type));

        // Undo shift
        lines.add(format!("v = v << {};", desc.shift));

        // To bytes
        lines.add("let buf = v.to_be_bytes();");

        // TODO: Usually this should be vectorizable as a full integer OR.
        lines.add(format!(
            "for i in 0..{} {{",
            desc.end_byte - desc.start_byte
        ));
        lines.add(format!(
            "bit_output[{} + i] |= buf[{} + i];",
            desc.start_byte,
            desc.int_size - (desc.end_byte - desc.start_byte)
        ));
        lines.add("}");

        lines.add("}");

        Ok(lines.to_string())
    }

    fn sizeof(&self, field_name: &str) -> Result<Option<SizeExpression>> {
        let n = match self.proto {
            PrimitiveTypeProto::UNKNOWN => {
                return Err(err_msg("No primitive type specified"));
            }
            PrimitiveTypeProto::U8 => 1,
            PrimitiveTypeProto::I8 => 1,
            PrimitiveTypeProto::U16 => 2,
            PrimitiveTypeProto::I16 => 2,
            PrimitiveTypeProto::U32 => 4,
            PrimitiveTypeProto::I32 => 4,
            PrimitiveTypeProto::U64 => 8,
            PrimitiveTypeProto::I64 => 8,
            PrimitiveTypeProto::FLOAT => 4,
            PrimitiveTypeProto::DOUBLE => 8,
            PrimitiveTypeProto::BOOL => 1, // TODO: Check this?
        };

        Ok(Some(SizeExpression::Constant(n)))
    }
}

/// Information about a field which is stored as a fraction of whole bytes.
struct BitField {
    int_size: usize,
    int_type: &'static str,

    /// Starting byte index in the input buffer of the first byte containing at
    /// least one bit of data for this field.
    start_byte: usize,

    /// Ending byte index after the last byte in the input buffer which contains
    /// at least one bit of data for this field.
    end_byte: usize,

    shift: usize,
    mask: String,
}

impl BitField {
    fn new(bit_offset: usize, bit_width: usize, ptype: PrimitiveTypeProto) -> Result<Self> {
        match ptype {
            PrimitiveTypeProto::U8
            | PrimitiveTypeProto::U16
            | PrimitiveTypeProto::U32
            | PrimitiveTypeProto::U64
            | PrimitiveTypeProto::BOOL => {}

            PrimitiveTypeProto::I8
            | PrimitiveTypeProto::I16
            | PrimitiveTypeProto::I32
            | PrimitiveTypeProto::I64
            | PrimitiveTypeProto::FLOAT
            | PrimitiveTypeProto::DOUBLE
            | PrimitiveTypeProto::UNKNOWN => {
                return Err(err_msg(
                    "Signed and float types not yet supported in bit fields",
                ));
            }
        };

        let start_byte = bit_offset / 8;
        let end_byte = common::ceil_div(bit_offset + bit_width, 8);

        // Need to find the next biggest integer to use as the intermediaum
        let (int_size, int_type) = {
            if end_byte - start_byte == 1 {
                (1, "u8")
            } else if end_byte - start_byte <= 2 {
                (2, "u16")
            } else if end_byte - start_byte <= 4 {
                (4, "u32")
            } else if end_byte - start_byte <= 8 {
                (8, "u64")
            } else {
                return Err(err_msg(
                    "Bit fields that span more than 8 bytes are not supported",
                ));
            }
        };

        let shift = end_byte * 8 - (bit_offset + bit_width);
        let mask = {
            let mut s = "0b".to_string();
            for i in 0..bit_width {
                s.push('1');
            }

            s
        };

        Ok(Self {
            int_size,
            int_type,
            shift,
            mask,
            start_byte,
            end_byte,
        })
    }
}
