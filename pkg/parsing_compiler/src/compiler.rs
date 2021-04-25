use std::collections::{HashMap, HashSet};
use common::line_builder::*;
use common::errors::*;
use crate::proto::dsl::*;

#[derive(Copy, Clone)]
enum NamedEntity<'a> {
    Struct(&'a Struct),
    Enum(&'a crate::proto::dsl::Enum)
}

/// Information about a field which is stored as a fraction of whole bytes.
struct BitField {
    int_size: usize,
    int_type: &'static str,

    /// Starting byte index in the input buffer of the first byte containing at least one bit of
    /// data for this field.
    start_byte: usize,

    /// Ending byte index after the last byte in the input buffer which contains at least one bit
    /// of data for this field. 
    end_byte: usize,

    shift: usize,
    mask: String
}

impl BitField {
    fn new(bit_offset: usize, bit_width: usize, ptype: PrimitiveType) -> Result<Self> {
        match ptype {
            PrimitiveType::U8 | PrimitiveType::U16 | PrimitiveType::U32 |
            PrimitiveType::U64 | PrimitiveType::BOOL => {},

            PrimitiveType::I8 | PrimitiveType::I16 | PrimitiveType::I32 |
            PrimitiveType::I64 | PrimitiveType::FLOAT |
            PrimitiveType::DOUBLE | PrimitiveType::UNKNOWN => {
                return Err(err_msg("Signed and float types not yet supported in bit fields"));
            },
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
                return Err(err_msg("Bit fields that span more than 8 bytes are not supported"));
            }
        };

        let shift = end_byte*8 - (bit_offset + bit_width);
        let mask = {
            let mut s = "0b".to_string();
            for i in 0..bit_width {
                s.push('1');
            }

            s
        };

        Ok(Self {
            int_size, int_type, shift, mask, start_byte, end_byte
        })
    }
}


pub struct Compiler<'a> {
    runtime_package: String,
    root: &'a BinaryDescriptorLibrary,
    index: HashMap<&'a str, NamedEntity<'a>>
}

/*
    Next steps:
    - Need bit fields.
    - Make sure that Vec<u8> becomes Bytes and ideally parses from Bytes directly without copies.

    - Need a golden based regression test.
    - Need to support parsing into a refernce

    - TODO: In some cases, if we have a union field, we may want to just store it as bytes and then later if we need to, it can lookup values as needed.
*/

impl<'c> Compiler<'c> {    
    pub fn compile(lib: &'c BinaryDescriptorLibrary, runtime_package: &str) -> Result<String> {
        let mut compiler = Self {
            runtime_package: runtime_package.to_owned(),
            root: lib,
            index: HashMap::new()
        };

        for s in lib.structs() {
            if !compiler.index.insert(s.name(), NamedEntity::Struct(s)).is_none() {
                return Err(format_err!("Duplicate entity named: {}", s.name()));
            }
        }

        for e in lib.enums() {
            if !compiler.index.insert(e.name(), NamedEntity::Enum(e)).is_none() {
                return Err(format_err!("Duplicate entity named: {}", e.name()));
            }
        }


        let mut lines = LineBuilder::new();
        lines.add("use ::common::errors::*;");
        lines.add("use ::parsing::parse_next;");

        for s in lib.structs() {
            lines.add(compiler.compile_struct(s)?);
        }

        for e in lib.enums() {
            lines.add(compiler.compile_enum(e)?);
        }

        Ok(lines.to_string())
    }

    fn compile_primitive_type(&self, typ: PrimitiveType) -> Result<&'static str> {
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
            PrimitiveType::BOOL => "bool"
        })
    }

    fn compile_type(&self, typ: &Type) -> Result<String> {
        Ok(match typ.type_case() {
            TypeTypeCase::Primitive(p) => self.compile_primitive_type(*p)?.to_string(),
            TypeTypeCase::Buffer(buf) => {
                let element_type = self.compile_type(&buf.element_type())?;

                match buf.size_case() {
                    BufferTypeSizeCase::FixedLength(len) => {
                        // TODO: Only do this up to some size threshold.
                        format!("[{}; {}]", element_type, len)
                    }
                    BufferTypeSizeCase::LengthFieldName(_) |
                    BufferTypeSizeCase::EndTerminated(_) => {
                        format!("Vec<{}>", element_type)
                    }
                    BufferTypeSizeCase::Unknown => {
                        return Err(err_msg("Unspecified buffer size"));
                    }
                }
            }
            TypeTypeCase::Named(name) => {
                // TODO: Must verify that this is a valid struct in the file (also sometimes can't be recursive).
                name.to_string()
            }
            TypeTypeCase::Unknown => {
                return Err(err_msg("Unspecified type"));
            }
        })
    }

    fn referenced_field_names<'a>(&self, typ: &'a Type) -> HashSet<&'a str> {
        let mut out = HashSet::new();

        fn recurse<'a>(t: &'a Type, out: &mut HashSet<&'a str>) {
            if let TypeTypeCase::Buffer(buf) = t.type_case() {
                if let BufferTypeSizeCase::LengthFieldName(name) = buf.size_case() {
                    out.insert(&name);
                }

                recurse(buf.element_type(), out);
            }
        }

        recurse(typ, &mut out);
        out
    }

    fn endian_str(&self, endian: Endian) -> Result<&'static str> {
        Ok(match endian {
            Endian::LITTLE_ENDIAN => "le",
            Endian::BIG_ENDIAN => "be",
            Endian::UNKNOWN => { return Err(err_msg("Unspecified endian")); } 
        })
    }

    fn compile_parse_bit_type(&self, typ: &Type, endian: Endian, bit_offset: usize, bit_width: usize) -> Result<String> {
        if endian != Endian::BIG_ENDIAN {
            return Err(err_msg("Bit fields only supported in big endian mode"));
        }

        let ptype = match typ.type_case() {
            TypeTypeCase::Primitive(p) => *p,
            _ => return Err(err_msg("Bit field must be a primitive"))
        };

        let desc = BitField::new(bit_offset, bit_width, ptype)?;

        
        let mut lines = LineBuilder::new();
        lines.add("{");
        lines.add(format!("let mut buf = [0u8; {}];", desc.int_size));

        // Copy the bytes into the buffer with it aligned as far to the right of the buffer as possible.
        lines.add(format!("(&mut buf[{}..{}]).copy_from_slice(&bit_input[{}..{}]);",
                  desc.int_size - (desc.end_byte - desc.start_byte), desc.int_size, desc.start_byte, desc.end_byte));
        
        lines.add(format!("let mut val = {}::from_be_bytes(buf);", desc.int_type));

        lines.add(format!("val = (val >> {}) & {};", desc.shift, desc.mask));

        // Now we'll cast it to the final type
        if ptype == PrimitiveType::BOOL {
            lines.add("val != 0");
        } else {
            lines.add(format!("(val as {})", self.compile_primitive_type(ptype)?));
        }

        lines.add("}");

        Ok(lines.to_string())
    }

    fn compile_serialize_bit_type(&self, typ: &Type, endian: Endian, bit_offset: usize, bit_width: usize, value: &str) -> Result<String> {
        if endian != Endian::BIG_ENDIAN {
            return Err(err_msg("Bit fields only supported in big endian mode"));
        }

        let ptype = match typ.type_case() {
            TypeTypeCase::Primitive(p) => *p,
            _ => return Err(err_msg("Bit field must be a primitive"))
        };

        let desc = BitField::new(bit_offset, bit_width, ptype)?;
        
        let mut lines = LineBuilder::new();
        lines.add("{");

        lines.add(format!("let raw_v = {};", value));
 
        // validate that the value in memory can fit into the serialized number of bits
        // without truncation.
        if ptype != PrimitiveType::BOOL {
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
        lines.add(format!("for i in 0..{} {{", desc.end_byte - desc.start_byte));
        lines.add(format!("bit_output[{} + i] |= buf[{} + i];",
            desc.start_byte, desc.int_size - (desc.end_byte - desc.start_byte)));
        lines.add("}"); 

        lines.add("}");

        Ok(lines.to_string())
    }

    /// Generates a string of code which evaluates to a parsed value of the type specified from
    /// an ambient buffer variable named 'input'. After the parsing is done, the code should also
    /// advance the 'input' buffer to the position after the value.
    ///
    /// TODO: For bit fields, this needs to be given a bit shift and mask to perform (only will work for primitives)
    fn compile_parse_type(
        &self, typ: &Type, endian: Endian, bit_slice: Option<(usize, usize)>,
        after_bytes: Option<usize>
    ) -> Result<String> {
        Ok(match typ.type_case() {
            TypeTypeCase::Primitive(p) => {
                if let Some((bit_offset, bit_width)) = bit_slice {
                    return self.compile_parse_bit_type(typ, endian, bit_offset, bit_width);
                }

                format!("parse_next!(input, ::parsing::binary::{}_{})",
                        self.endian_str(endian)?, self.compile_primitive_type(*p)?)
            },
            TypeTypeCase::Buffer(buf) => {

                // TODO: Other important cases:
                // - Sometimes want to support zero-copy access
                // - If the endian is the same as the host, then we don't need to perform any individual element parsing.

                // Step 1: allocate memory given that we should know the length and then assign or push to that.
                // In some cases, we may want to give the inner parser a mutable reference to improve efficiency? 

                let element_parser = self.compile_parse_type(&buf.element_type(), endian, None, None)?;

                let mut lines = LineBuilder::new();
                lines.add("{");

                match buf.size_case() {
                    // TODO: Some types like large primitive slices can be optimized.
                    BufferTypeSizeCase::FixedLength(len) => {
                        lines.add(format!("\tlet mut buf = [{}::default(); {}];", self.compile_type(buf.element_type())?, len));
                        
                        if let TypeTypeCase::Primitive(PrimitiveType::U8) = typ.type_case() {
                            // TODO: Ensure that we always take exact a slice (and not Bytes as that is an expensive copy)!
                            lines.add("\tbuf.copy_from_slice(parse_next!(input, ::parsing::take_exact(buf.len())));");
                        } else {
                            lines.add("\tfor i in 0..buf.len() {");
                            lines.add(format!("\t\tbuf[i] = {};", element_parser));
                            lines.add("\t}");
                        }
                    }
                    BufferTypeSizeCase::LengthFieldName(name) => {
                        lines.add("\tlet mut buf = vec![];");
                        
                        if let TypeTypeCase::Primitive(PrimitiveType::U8) = typ.type_case() {
                            lines.add(format!("\tbuf.extend_from_slice(parse_next!(input, ::parsing::take_exact({})));", name));
                        } else {
                            lines.add(format!("\tbuf.reserve({}_value as usize);", name));
                            lines.add(format!("\tfor _ in 0..{}_value {{", name));
                            lines.add(format!("\t\tbuf.push({});", element_parser));
                            lines.add("\t}");
                        }
                    }
                    BufferTypeSizeCase::EndTerminated(_) => {
                        let after_count = after_bytes.ok_or(
                            err_msg("end_terminated buffer not validated"))?;

                        lines.add("\tlet mut buf = vec![];");
                        lines.add(format!("\tlet length = input.len() - {};", after_count));

                        if let TypeTypeCase::Primitive(PrimitiveType::U8) = typ.type_case() {
                            lines.add("\tbuf.extend_from_slice(parse_next!(input, ::parsing::take_exact(length)));");
                        } else {
                            lines.add("\tbuf.reserve(length);");
                            lines.add("\tfor _ in 0..length {");
                            lines.add(format!("\t\tbuf.push({});", element_parser));
                            lines.add("\t}");
                        }
                    }
                    BufferTypeSizeCase::Unknown => { panic!(); }
                }

                lines.add("\tbuf");
                lines.add("}");
                lines.indent();
                lines.indent();

                lines.to_string()
            }
            TypeTypeCase::Named(name) => {
                format!("parse_next!(input, {}::parse)", name)
            }
            TypeTypeCase::Unknown => {
                return Err(err_msg("Unspecified type"));
            }
        })
    }

    // TODO: We don't use after_bytes for this!!
    fn compile_serialize_type(
        &self, typ: &Type, value: &str, endian: Endian, bit_slice: Option<(usize, usize)>,
    ) -> Result<String> {
        if let Some((bit_offset, bit_width)) = bit_slice {
            return self.compile_serialize_bit_type(typ, endian, bit_offset, bit_width, value);
        }

        Ok(match typ.type_case() {
            TypeTypeCase::Primitive(p) => {
                format!("out.extend_from_slice(&{}.to_{}_bytes());", value, self.endian_str(endian)?)
            },
            TypeTypeCase::Buffer(buf) => {
                // Optimized case for [u8]
                if let TypeTypeCase::Primitive(PrimitiveType::U8) = buf.element_type().type_case() {
                    return Ok(format!("out.extend_from_slice(&{});", value));
                }

                let mut lines = LineBuilder::new();
                lines.add(format!("for item in &{} {{", value));
                lines.add(format!("\t{}", self.compile_serialize_type(buf.element_type(), "item", endian, None)?));
                lines.add("}");
                lines.to_string()
            }
            TypeTypeCase::Named(name) => {
                format!("{}.serialize(out);", value)
            }
            TypeTypeCase::Unknown => {
                return Err(err_msg("Unspecified type"));
            }
        })
    }

    // Hello world this is a test of the new keycaps and how well they work compared to the old keycaps. THe anser
    // Is that it's pretty good and there is generally no issues with it so that's great.

    /// If statically known, then will get the length of the given type in bytes.
    ///
    /// NOTE: This won't return the correct value if this is the type of a field using 
    /// This is used primarily 
    fn sizeof_type(&self, typ: &Type) -> Result<Option<usize>> {
        Ok(match typ.type_case() {
            TypeTypeCase::Primitive(p) => {
                Some(match p {
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
            TypeTypeCase::Buffer(buf) => {
                let maybe_element_size = self.sizeof_type(buf.element_type())?;
                if let Some(element_size) = maybe_element_size {
                    // TODO: If we have already parsed the 
                    if let BufferTypeSizeCase::FixedLength(len) = buf.size_case() {
                        return Ok(Some((*len as usize) * element_size));
                    }
                }

                None
            }
            TypeTypeCase::Named(name) => {
                let entity = self.index.get(name.as_str())
                    .map(|v| *v)
                    .ok_or_else(|| format_err!("No entity named: {}", name))?;
                
                match entity {
                    NamedEntity::Enum(e) => {
                        return self.sizeof_type(e.typ());
                    }
                    NamedEntity::Struct(s) => {
                        // TODO: Ideally we should cache these.

                        let mut total_bytes = 0;
                        let mut bits = 0;

                        for field in s.field() {
                            if field.bit_width() > 0 {
                                bits += field.bit_width() as usize;
                            } else {
                                let field_size = {
                                    if let Some(v) = self.sizeof_type(field.typ())? {
                                        v
                                    } else {
                                        return Ok(None);
                                    }
                                };

                                total_bytes += field_size;
                            }
                        }

                        total_bytes += bits / 8;
                        Some(total_bytes)
                    }
                }
            }
            TypeTypeCase::Unknown => {
                return Err(err_msg("Unspecified type"));
            }
        })
    }

    fn compile_struct(&self, desc: &Struct) -> Result<String> {
        let mut lines = LineBuilder::new();

        // All fields in the struct indexed by name.
        let mut field_index: HashMap<&str, &Field> = HashMap::new();

        // Fields which are used to indicate the size of another field so aren't directly defined as a struct field.
        let mut derivated_fields: HashSet<&str> = HashSet::new();

        for field in desc.field() {
            if field_index.insert(&field.name(), field).is_some() {
                return Err(err_msg("Duplicate field"));
            }

            let used_names = self.referenced_field_names(field.typ());

            for name in &used_names {
                if !field_index.contains_key(name) {
                    // TODO: If a length field is used in multiple different fields, then we need to do validation at serialization time that sizes are correct.
                    // TODO: Eventually support reading fields from the back of a struct in some cases.
                    return Err(err_msg("Field referenced before parsed"));
                }
            }

            derivated_fields.extend(&used_names);
        }

        // For each first bit field in a sequence, this will store the number of bits 
        let mut bit_field_spans: HashMap<&str, usize> = HashMap::new();

        // Validate that all bit fields align together to 8-bit boundaries
        // NOTE: We don't support interleaving non-bit fields with bit fields unless the non-bit field
        // starts at a 8-bit boundary.
        {
            let mut num_bits: Option<(&str, usize)> = None;
            for field in desc.field() {
                if field.bit_width() > 0 {
                    num_bits = Some(num_bits.map_or(
                        (field.name(), field.bit_width() as usize),
                         |(name, nbits)| (name, nbits + field.bit_width() as usize)));

                    // Only primitive fields can be used as bit fields.
                    if let TypeTypeCase::Primitive(_) = field.typ().type_case() {
                        // All good
                        // TODO: Must also validate that the given primitive type can fit the num bits.
                    } else {
                        return Err(err_msg("Only primitive bit fields are currently supported"));
                    }
                } else if let Some((first_field, nbits)) = num_bits.take() {
                    if nbits % 8 != 0 {
                        return Err(err_msg("Bits do not align to whole byte offsets"));
                    }

                    // TODO: This is no longer unique if there are 
                    bit_field_spans.insert(first_field, nbits);
                }
            }

            // TODO: Deduplicate with above.
            if let Some((first_field, nbits)) = num_bits.take() {
                if nbits % 8 != 0 {
                    return Err(format_err!("Bits do not align to whole byte offsets. Total: {}", nbits));
                }

                bit_field_spans.insert(first_field, nbits);
            }
        }

        let mut end_terminated_marker = None;
        {
            let mut end_bits = 0;
            let mut well_defined = true;

            for field in desc.field().iter().rev() {
                if let TypeTypeCase::Buffer(b) = field.typ().type_case() {
                    if let BufferTypeSizeCase::EndTerminated(is_end_terminated) = b.size_case() {
                        if !is_end_terminated {
                            return Err(err_msg("end_terminated field present but not true"));
                        }

                        if !well_defined {
                            return Err(err_msg(
                                "end_terminated buffer doesn't have a well defined number of bytes following it."));
                        }

                        end_terminated_marker = Some((field.name(), (end_bits / 8) as usize));
                        well_defined = false;
                    }
                }
                
                if field.bit_width() > 0 {
                    end_bits += field.bit_width() as usize;
                } else if let Some(byte_size) = self.sizeof_type(field.typ())? {
                    end_bits += byte_size * 8;
                } else {
                    well_defined = false;
                }
            }
        }

        // TODO: Consider using packed memory?
        lines.add("#[derive(Debug, PartialEq)]");
        lines.add(format!("pub struct {} {{", desc.name()));

        // Adding struct member delarations.
        for field in desc.field() {
            if derivated_fields.contains(field.name()) {
                if let TypeTypeCase::Primitive(_) = field.typ().type_case() {
                    // All is good.
                } else {
                    // TODO: We should be more specific. Only allow unsigned integer types?
                    return Err(err_msg("Expected length fields to have scaler types"));
                }

                continue;
            }

            let typename = self.compile_type(field.typ())?;
            if !field.comment().is_empty() {
                lines.add(format!("\t/// {}", field.comment()));
            }
            lines.add(format!("\tpub {}: {},", field.name(), typename));
        }

        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", desc.name()));

        // Add accessors for derived fuekds,
        //
        // TODO: If a length field is referenced multiple times, then we need to verify that all vectors have consistent length.
        // TODO: Also if the size is used as an inner dimension of a buffer, then we can't determin
        for field in desc.field() {
            if let TypeTypeCase::Buffer(buf) = field.typ().type_case() {
                if let BufferTypeSizeCase::LengthFieldName(name) = buf.size_case() {
                    // TODO: Challenge here is that we must ensure that the size fits within the limits of the type (no overflows when serializing).
                    let size_type = self.compile_type(field_index.get(name.as_str()).unwrap().typ())?;
                    lines.add(format!("\tpub fn {}(&self) -> {} {{ self.{}.len() as {} }}", name, size_type, field.name(), size_type));
                }
            }
        }


        // Also need to support parsing from Bytes to have fewer copies.
        lines.add("\tpub fn parse(mut input: &[u8]) -> Result<(Self, &[u8])> {");
        {
            let mut bit_offset = 0;

            for field in desc.field() {
                let mut bit_slice = None;
                if field.bit_width() > 0 {
                    if let Some(span_width) = bit_field_spans.get(field.name()).cloned() {
                        // This is the first field in the bit slice, so we'll parse the full slice.
                        bit_offset = 0;
                        lines.add(format!("let bit_input = &input[0..{}];", span_width / 8));
                        lines.add("input = &input[bit_input.len()..];");
                    }

                    bit_slice = Some((bit_offset, (field.bit_width() as usize)));
                    bit_offset += field.bit_width() as usize;
                }


                let after_bytes = end_terminated_marker.and_then(|(name, bytes)| {
                    if name == field.name() {
                        Some(bytes)
                    } else {
                        None
                    }
                });

                // May need to add the end_terminated_

                lines.add(format!("\t\tlet {}_value = {};",
                        field.name(), self.compile_parse_type(field.typ(), desc.endian(), bit_slice, after_bytes)?));
            }
            lines.nl();

            lines.add(format!("\t\tOk(({} {{", desc.name()));
            for field in desc.field() {
                if derivated_fields.contains(field.name()) {
                    continue;
                }

                lines.add(format!("\t\t\t{}: {}_value,", field.name(), field.name()));
            }
            lines.add("\t\t}, input))");

        }
        lines.add("\t}");
        lines.nl();

        lines.add("\tpub fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {");
        {
            // TODO: Need to support lots of exotic derived fields.

            let mut bit_offset = 0;

            for field in desc.field() {
                let mut bit_slice = None;
                if field.bit_width() > 0 {
                    if let Some(span_width) = bit_field_spans.get(field.name()).cloned() {
                        // This is the first field in the bit slice, so we'll allocate some space for the
                        // entire slice and then we'll use it.
                        bit_offset = 0;

                        lines.add("let bit_output = {");
                        lines.add("let start = out.len();");
                        lines.add(format!("out.resize(start + {}, 0);", span_width / 8));
                        lines.add(format!("&mut out[start..(start + {})]", span_width / 8));
                        lines.add("};");
                    }

                    bit_slice = Some((bit_offset, (field.bit_width() as usize)));
                    bit_offset += field.bit_width() as usize;
                }

                let line = if derivated_fields.contains(field.name()) {
                    self.compile_serialize_type(
                        field.typ(), &format!("self.{}()", field.name()), desc.endian(),
                        bit_slice)?
                } else {
                    self.compile_serialize_type(
                        field.typ(), &format!("self.{}", field.name()), desc.endian(),
                        bit_slice)?
                };

                lines.add(format!("\t\t{}", line));
            }

            lines.add("\t\tOk(())");
        }
        lines.add("\t}");


        lines.add("}");

        // Now we need a parse and serialize routine.

        Ok(lines.to_string())
    }

    // TODO: How do we support enums which are BitFields.
    // Needs to be for both the input and output stage.
    fn compile_enum(&self, desc: &crate::proto::dsl::Enum) -> Result<String> {
        let mut lines = LineBuilder::new();

        let raw_type = self.compile_type(desc.typ())?;

        lines.add("#[derive(Debug)]");
        lines.add(format!("pub enum {} {{", desc.name()));
        for value in desc.values() {
            if !value.comment().is_empty() {
                lines.add(format!("\t/// {}", value.comment()));
            }
            lines.add(format!("\t{},", value.name()));
        }
        lines.add(format!("\tUnknown({})", raw_type));
        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", desc.name()));
        lines.indented(|lines| -> Result<()> {
            lines.add("pub fn parse(mut input: &[u8]) -> Result<(Self, &[u8])> {");
            lines.add(format!("\tlet value = {};", self.compile_parse_type(desc.typ(), desc.endian(), None, None)?));
            lines.add("\tlet inst = match value {");
            for value in desc.values() {
                lines.add(format!("\t\t{} => {}::{},", value.value(), desc.name(), value.name()));
            }
            lines.add(format!("\t\tv @ _ => {}::Unknown(v)", desc.name()));
            lines.add("\t};");
            lines.nl();

            lines.add("\tOk((inst, input))");
            lines.add("}");
            lines.nl();

            lines.add("pub fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {");
            lines.add(format!("\tlet value: {} = match self {{", raw_type));
            for value in desc.values() {
                lines.add(format!("\t\t{}::{} => {},", desc.name(), value.name(), value.value()));
            }
            lines.add(format!("\t\t{}::Unknown(v) => *v", desc.name()));
            lines.add("\t};");
            lines.nl();

            lines.add(self.compile_serialize_type(desc.typ(), "value", desc.endian(), None)?);

            lines.add("\tOk(())");
            lines.add("}");

            Ok(())
        })?;
        lines.add("}");
        lines.nl();

        Ok(lines.to_string())
    }
}