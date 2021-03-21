use std::collections::{HashMap, HashSet};
use common::line_builder::*;
use common::errors::*;
use crate::proto::dsl::*;

pub struct Compiler {
    runtime_package: String,
}

impl Compiler {    
    pub fn compile(lib: &BinaryDescriptorLibrary, runtime_package: &str) -> Result<String> {
        let mut compiler = Self { runtime_package: runtime_package.to_owned() };

        let mut lines = LineBuilder::new();

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
            PrimitiveType::DOUBLE => "f64"
        })
    }

    fn compile_type(&self, typ: &Type) -> Result<String> {
        Ok(match typ.type_case() {
            TypeTypeCase::Primitive(p) => self.compile_primitive_type(*p)?.to_string(),
            TypeTypeCase::Buffer(buf) => {
                let element_type = self.compile_type(&buf.element_type())?;

                match buf.size_case() {
                    BufferTypeSizeCase::FixedLength(len) => {
                        format!("[{}; {}]", element_type, len)
                    }
                    BufferTypeSizeCase::LengthFieldName(_) => {
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

    fn compile_parse_type(&self, typ: &Type, endian: Endian) -> Result<String> {
        Ok(match typ.type_case() {
            TypeTypeCase::Primitive(p) => {
                // TODO: Support different endian.
                format!("parse_next!(input, ::parsing::binary::{}_{})",
                        self.endian_str(endian)?, self.compile_primitive_type(*p)?)
            },
            TypeTypeCase::Buffer(buf) => {
                // Step 1: allocate memory given that we should know the length and then assign or push to that.
                // In some cases, we may want to give the inner parser a mutable reference to improve efficiency? 

                let element_parser = self.compile_parse_type(&buf.element_type(), endian)?;

                let mut lines = LineBuilder::new();
                lines.add("{");

                match buf.size_case() {
                    // TODO: Some types like large primitive slices can be optimized.
                    BufferTypeSizeCase::FixedLength(len) => {
                        lines.add(format!("\tlet mut buf = [{}::default(); {}];", self.compile_type(buf.element_type())?, len));
                        lines.add("\tfor i in 0..buf.len() {");
                        lines.add(format!("\t\tbuf[i] = {};", element_parser));
                        lines.add("\t}");
                        lines.add("\tbuf");
                    }
                    BufferTypeSizeCase::LengthFieldName(name) => {
                        lines.add("\tlet mut buf = vec![];");
                        lines.add(format!("\tbuf.reserve({}_value as usize);", name));
                        lines.add(format!("\tfor i in 0..{}_value {{", name));
                        lines.add(format!("\t\tbuf.push({});", element_parser));
                        lines.add("\t}");
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

    fn compile_serialize_type(&self, typ: &Type, value: &str, endian: Endian) -> Result<String> {
        Ok(match typ.type_case() {
            TypeTypeCase::Primitive(p) => {
                format!("out.extend_from_slice(&{}.to_{}_bytes());", value, self.endian_str(endian)?)
            },
            TypeTypeCase::Buffer(buf) => {
                let mut lines = LineBuilder::new();
                lines.add(format!("for item in &{} {{", value));
                lines.add(format!("\t{}", self.compile_serialize_type(buf.element_type(), "item", endian)?));
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

        // TODO: Consider using packed memory?
        lines.add("#[derive(Debug)]");
        lines.add(format!("pub struct {} {{", desc.name()));

        for field in desc.field() {
            if derivated_fields.contains(field.name()) {

                if let TypeTypeCase::Primitive(_) = field.typ().type_case() {

                } else {
                    // TODO: We should be more specific. Only allow unsigned integer types?
                    return Err(err_msg("Expected length fields to have scaler types"));
                }

                continue;
            }

            let typename = self.compile_type(field.typ())?;
            lines.add(format!("\tpub {}: {},", field.name(), typename));
        }

        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", desc.name()));

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


        lines.add("\tpub fn parse(mut input: &[u8]) -> Result<(Self, &[u8])> {");
        {
            for field in desc.field() {
                lines.add(format!("\t\tlet {}_value = {};",
                          field.name(), self.compile_parse_type(field.typ(), desc.endian())?));
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

            for field in desc.field() {
                let line = if derivated_fields.contains(field.name()) {
                    self.compile_serialize_type(field.typ(), &format!("self.{}()", field.name()), desc.endian())?
                } else {
                    self.compile_serialize_type(field.typ(), &format!("self.{}", field.name()), desc.endian())?
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

    fn compile_enum(&self, desc: &crate::proto::dsl::Enum) -> Result<String> {
        let mut lines = LineBuilder::new();

        let raw_type = self.compile_type(desc.typ())?;

        lines.add("#[derive(Debug)]");
        lines.add(format!("pub enum {} {{", desc.name()));
        for value in desc.values() {
            lines.add(format!("\t{},", value.name()));
        }
        lines.add(format!("\tUnknown({})", raw_type));
        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", desc.name()));
        lines.indented(|lines| -> Result<()> {
            lines.add("pub fn parse(mut input: &[u8]) -> Result<(Self, &[u8])> {");
            lines.add(format!("\tlet value = {};", self.compile_parse_type(desc.typ(), desc.endian())?));
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

            lines.add(self.compile_serialize_type(desc.typ(), "value", desc.endian())?);

            lines.add("\tOk(())");
            lines.add("}");

            Ok(())
        })?;
        lines.add("}");
        lines.nl();

        Ok(lines.to_string())
    }
}