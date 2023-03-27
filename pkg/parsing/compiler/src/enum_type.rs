use std::collections::HashMap;

use common::errors::*;
use common::line_builder::*;

use crate::expression::Expression;
use crate::expression::*;
use crate::proto::*;
use crate::types::*;

pub struct EnumType<'a> {
    proto: &'a EnumTypeProto,

    /// Instantiated using proto.typ()
    inner_typ: TypeReference<'a>,
}

impl<'a> EnumType<'a> {
    pub fn create(proto: &'a EnumTypeProto, resolver: &mut TypeResolver<'a>) -> Result<Self> {
        let inner_typ = resolver.resolve_type(
            proto.typ(),
            &TypeResolverContext {
                endian: proto.endian(),
            },
        )?;

        Ok(Self { proto, inner_typ })
    }
}

impl<'a> Type for EnumType<'a> {
    // TODO: How do we support enums which are BitFields.
    // Needs to be for both the input and output stage.
    fn compile_declaration(&self, lines: &mut LineBuilder) -> Result<()> {
        let raw_type = self.inner_typ.get().type_expression()?;

        lines.add("#[derive(Debug, Clone, Copy)]");
        lines.add(format!("pub enum {} {{", self.proto.name()));
        for value in self.proto.values() {
            if !value.comment().is_empty() {
                lines.add(format!("\t/// {}", value.comment()));
            }
            lines.add(format!("\t{},", value.name()));
        }
        if !self.proto.exhaustive() {
            lines.add(format!("\tUnknown({})", raw_type));
        }
        lines.add("}");
        lines.nl();

        lines.add(format!("impl Default for {} {{", self.proto.name()));
        let first_case = self.proto.values()[0].name();
        lines.add(format!("fn default() -> Self {{ Self::{} }}", first_case));
        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", self.proto.name()));
        lines.indented(|lines| -> Result<()> {
            lines.add("pub fn parse(mut input: &[u8]) -> Result<(Self, &[u8])> {");
            lines.add(format!(
                "\tlet value = {};",
                self.inner_typ
                    .get()
                    .parse_bytes_expression(&TypeParserContext {
                        stream: "input".to_string(),
                        after_bytes: None,
                        arguments: &HashMap::new()
                    })?
            ));
            lines.add(format!("\tlet inst = Self::from_{}(value)?;", raw_type));

            lines.add("\tOk((inst, input))");
            lines.add("}");
            lines.nl();

            lines.add(format!(
                "pub fn from_{}(value: {}) -> Result<Self> {{",
                raw_type, raw_type
            ));

            lines.add("\tOk(match value {");
            for value in self.proto.values() {
                lines.add(format!(
                    "\t\t{} => {}::{},",
                    value.value(),
                    self.proto.name(),
                    value.name()
                ));
            }
            if self.proto.exhaustive() {
                lines.add(format!(
                    r#"\t\tv @ _ => return Err(format_err!("Unknown value of {}: {{}}", v))"#,
                    self.proto.name()
                ));
            } else {
                lines.add(format!("\t\tv @ _ => {}::Unknown(v)", self.proto.name()));
            }

            lines.add("\t})");
            lines.add("}");
            lines.nl();

            lines.add("pub fn serialize(&self, out: &mut Vec<u8>) -> Result<()> {");
            lines.add(format!("\tlet value = self.to_{}();", raw_type));
            lines.add(self.inner_typ.get().serialize_bytes_expression(
                "value",
                &TypeParserContext {
                    stream: "out".to_string(),
                    after_bytes: None,
                    arguments: &HashMap::new(),
                },
            )?);
            lines.add("\tOk(())");
            lines.add("}");
            lines.nl();

            lines.add(format!("pub fn to_{}(&self) -> {} {{", raw_type, raw_type));
            lines.add("\tmatch self {");
            for value in self.proto.values() {
                lines.add(format!(
                    "\t\t{}::{} => {},",
                    self.proto.name(),
                    value.name(),
                    value.value()
                ));
            }
            lines.add(format!("\t\t{}::Unknown(v) => *v", self.proto.name()));
            lines.add("\t}");
            lines.add("}");

            Ok(())
        })?;
        lines.add("}");
        lines.nl();

        // NOTE: This custom PartialEq is used to ensure that an Unknown(x) value equals
        // the known variation of it.
        lines.add(format!(
            "impl ::std::cmp::PartialEq for {} {{",
            self.proto.name()
        ));
        lines.indented(|lines| {
            lines.add("fn eq(&self, other: &Self) -> bool {");
            lines.add(format!(
                "\tself.to_{}() == other.to_{}()",
                raw_type, raw_type
            ));
            lines.add("}");
        });
        lines.add("}");
        lines.nl();

        lines.add(format!(
            "impl ::std::cmp::Eq for {} {{}}",
            self.proto.name()
        ));
        lines.nl();

        lines.add(format!(
            "impl ::std::hash::Hash for {} {{",
            self.proto.name()
        ));
        lines.add("\tfn hash<H: ::std::hash::Hasher>(&self, state: &mut H) {");
        lines.add(format!(
            "\t\t::std::hash::Hash::hash(&self.to_{}(), state);",
            raw_type
        ));
        lines.add("\t}");
        lines.add("}");

        Ok(())
    }

    fn type_expression(&self) -> Result<String> {
        Ok(self.proto.name().to_string())
    }

    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String> {
        Ok(format!("parse_next!(input, {}::parse)", self.proto.name()))
    }

    fn parse_bits_expression(&self, bit_offset: usize, bit_width: usize) -> Result<String> {
        let inner_value = self
            .inner_typ
            .get()
            .parse_bits_expression(bit_offset, bit_width)?;
        let raw_type = self.inner_typ.get().type_expression()?;

        // TODO: Must check the enum is compatible with the minimally sized int type
        Ok(format!(
            "{}::from_{}({})?",
            self.proto.name(),
            raw_type,
            inner_value
        ))
    }

    fn serialize_bytes_expression(
        &self,
        value: &str,
        context: &TypeParserContext,
    ) -> Result<String> {
        Ok(format!("{}.serialize({})?;", value, context.stream))
    }

    fn serialize_bits_expression(
        &self,
        value: &str,
        bit_offset: usize,
        bit_width: usize,
    ) -> Result<String> {
        // TODO: This assumes that the enum is stored as the minimal integer type needed
        // for the number of bits.
        let raw_type = self.inner_typ.get().type_expression()?;
        self.inner_typ.get().serialize_bits_expression(
            &format!("{}.to_{}()", value, raw_type),
            bit_offset,
            bit_width,
        )
    }

    fn size_of(&self, field_name: &str) -> Result<Option<Expression>> {
        self.inner_typ.get().size_of(field_name)
    }
}
