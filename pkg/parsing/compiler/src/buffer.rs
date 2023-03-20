use common::errors::*;
use common::line_builder::*;

use crate::expression::Expression;
use crate::expression::*;
use crate::proto::*;
use crate::types::*;

pub struct BufferType<'a> {
    proto: &'a BufferTypeProto,

    // Instantiated from proto.element_type()
    element_type: TypeReference<'a>,
}

impl<'a> BufferType<'a> {
    pub fn create(
        proto: &'a BufferTypeProto,
        resolver: &mut TypeResolver<'a>,
        context: &TypeResolverContext,
    ) -> Result<Self> {
        let element_type = resolver.resolve_type(proto.element_type(), context)?;
        Ok(Self {
            proto,
            element_type,
        })
    }
}

impl<'a> Type for BufferType<'a> {
    fn compile_declaration(&self, out: &mut LineBuilder) -> Result<()> {
        Ok(())
    }

    fn type_expression(&self) -> Result<String> {
        let element_type = self.element_type.get().type_expression()?;

        Ok(match self.proto.size_case() {
            BufferTypeProtoSizeCase::FixedLength(len) => {
                // TODO: Only do this up to some size threshold.
                format!("[{}; {}]", element_type, len)
            }
            BufferTypeProtoSizeCase::LengthFieldName(_)
            | BufferTypeProtoSizeCase::EndTerminated(_)
            | BufferTypeProtoSizeCase::EndMarker(_) => {
                format!("Vec<{}>", element_type)
            }
            BufferTypeProtoSizeCase::Unknown => {
                return Err(err_msg("Unspecified buffer size"));
            }
        })
    }

    fn default_value_expression(&self) -> Result<String> {
        let element_default = self.element_type.get().default_value_expression()?;

        Ok(match self.proto.size_case() {
            BufferTypeProtoSizeCase::Unknown => todo!(),
            BufferTypeProtoSizeCase::FixedLength(len) => {
                let mut parts = vec![];
                for i in 0..*len {
                    parts.push(element_default.as_str());
                }

                format!("[{}]", parts.join(","))
            }
            BufferTypeProtoSizeCase::LengthFieldName(_)
            | BufferTypeProtoSizeCase::EndTerminated(_)
            | BufferTypeProtoSizeCase::EndMarker(_) => {
                format!("vec![]")
            }
        })
    }

    fn value_expression(&self, value: &Value) -> Result<String> {
        if value.int64_value().len() > 0 {
            // TODO: This is only valid for fixed length fields.
            return Ok(format!(
                "[{}]",
                value
                    .int64_value()
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }

        Err(err_msg("Unsupported value type"))
    }

    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String> {
        // TODO: Other important cases:
        // - Sometimes want to support zero-copy access
        // - If the endian is the same as the host, then we don't need to perform any
        //   individual element parsing.

        // Step 1: allocate memory given that we should know the length and then assign
        // or push to that. In some cases, we may want to give the inner
        // parser a mutable reference to improve efficiency?

        let element_parser = self.element_type.get().parse_bytes_expression(context)?;

        let mut lines = LineBuilder::new();
        lines.add("{");

        match self.proto.size_case() {
            // TODO: Some types like large primitive slices can be optimized.
            BufferTypeProtoSizeCase::FixedLength(len) => {
                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    lines.add(format!(
                        "\tlet mut buf = [{}::default(); {}];",
                        self.element_type.get().type_expression()?,
                        len
                    ));

                    // TODO: Ensure that we always take exact a slice (and not Bytes as that
                    // is an expensive copy)!
                    lines.add("\t{ let n = buf.len(); buf.copy_from_slice(parse_next!(input, ::parsing::take_exact(n))); }");
                    lines.add("\tbuf");
                } else {
                    lines.add(format!(
                                "\tlet mut buf: [core::mem::MaybeUninit<{}>; {}] = core::mem::MaybeUninit::uninit_array();",
                                self.element_type.get().type_expression()?,
                                len
                            ));

                    lines.add("\tfor i in 0..buf.len() {");
                    lines.add(format!("\t\tbuf[i].write({});", element_parser));
                    lines.add("\t}");

                    lines.add("\tunsafe {  core::mem::MaybeUninit::array_assume_init(buf) }");
                }
            }
            BufferTypeProtoSizeCase::LengthFieldName(name) => {
                lines.add("\tlet mut buf = vec![];");

                // TODO: Have a better way of handling this.
                // Maybe require that both fields have matching presence expressions?
                let len_expr = {
                    context.arguments.get(name.as_str()).unwrap()

                    // let field = context.scope.get(name.as_str()).unwrap();
                    // if !field.proto.presence().is_empty() {
                    //     format!("{}_value.unwrap_or(0) as usize", name)
                    // } else {
                    //     format!("{}_value as usize", name)
                    // }
                };

                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    lines.add(format!(
                        "\tbuf.extend_from_slice(parse_next!(input, ::parsing::take_exact({} as usize)));",
                        len_expr
                    ));
                } else {
                    lines.add(format!("\tbuf.reserve({} as usize);", len_expr));
                    lines.add(format!("\tfor _ in 0..{}_value {{", name));
                    lines.add(format!("\t\tbuf.push({});", element_parser));
                    lines.add("\t}");
                }

                lines.add("\tbuf");
            }
            BufferTypeProtoSizeCase::EndTerminated(_) => {
                let after_count = context
                    .after_bytes
                    .as_ref()
                    .ok_or(err_msg("end_terminated buffer not validated"))?;

                lines.add("\tlet mut buf = vec![];");

                // TODO: Fix the remaining_bytes value used.
                lines.add(format!(
                    r#"
                            let length = input.len().checked_sub({})
                                .ok_or_else(|| ::parsing::incomplete_error(0))?;"#,
                    after_count
                ));

                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    lines.add("\tbuf.extend_from_slice(parse_next!(input, ::parsing::take_exact(length)));");
                } else {
                    lines.add(format!(
                        r#"{{
                                let mut input = parse_next!(input, ::parsing::take_exact(length));
                                while !input.is_empty() {{
                                    buf.push({});
                                }}
                            }}"#,
                        element_parser
                    ));
                }

                lines.add("\tbuf");
            }
            BufferTypeProtoSizeCase::EndMarker(marker) => {
                lines.add("{");
                lines.add("\tlet mut buf = vec![];");

                lines.add(format!(
                    "const MARKER: &'static [u8] = &{:?};",
                    marker.as_ref()
                ));
                lines.add(r#"
                    let mut input = parse_next!(input, |i| ::parsing::search::parse_pattern_terminated_bytes(i, MARKER));
                "#);

                // TODO: We may be able to reserve memory if we know the size of each element.

                // TODO: Deduplicate this.
                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    lines.add("\tbuf.extend_from_slice(input);");
                } else {
                    lines.add(format!(
                        r#"{{
                            while !input.is_empty() {{
                                buf.push({});
                            }}
                        }}"#,
                        element_parser
                    ));
                }

                lines.add("\tbuf");

                lines.add("}");
            }
            BufferTypeProtoSizeCase::Unknown => {
                panic!();
            }
        }

        lines.add("}");
        lines.indent();
        lines.indent();

        Ok(lines.to_string())
    }

    fn serialize_bytes_expression(
        &self,
        value: &str,
        output_buffer: &str,
        context: &TypeParserContext,
    ) -> Result<String> {
        let mut lines = LineBuilder::new();

        lines.add("{");

        // TODO: If the length is not know ahead of time, serialize an empty length
        // field and later fix it.

        if let BufferTypeProtoSizeCase::LengthFieldName(field) = self.proto.size_case() {
            let len_value = context
                .arguments
                .get(field.as_str())
                .ok_or_else(|| err_msg("Length field not fed as argument"))?;

            lines.add(format!(
                r#"
                if {} as usize != ({}).len() {{
                    return Err(err_msg("Data length does not match length field"));
                }}
            "#,
                len_value, value
            ));
        }

        lines.add("let start_i = out.len();");

        // Optimized case for [u8]
        if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
            self.proto.element_type().type_case()
        {
            lines.add(format!("out.extend_from_slice(&{});", value));
        } else {
            lines.add(format!("for item in {}.iter() {{", value));
            lines.add(format!(
                "\t{}",
                self.element_type
                    .get()
                    .serialize_bytes_expression("item", output_buffer, context)?
            ));
            lines.add("}");
        }

        if let BufferTypeProtoSizeCase::EndMarker(marker) = self.proto.size_case() {
            lines.add("let end_i = out.len();");

            lines.add(format!(
                "const MARKER: &'static [u8] = &{:?};",
                marker.as_ref()
            ));
            lines.add("out.extend_from_slice(MARKER);");

            lines.add(
                r#"
            if ::parsing::search::find_byte_pattern(&out[start_i..], MARKER) != Some(end_i)  {
                return Err(err_msg("Data contains end marker"));
            }
            "#,
            );
        }

        lines.add("}");

        Ok(lines.to_string())
    }

    // TODO: This will have a well defined length if we can reference a field
    // (unfortunately this is harder for nested fields).
    fn size_of(&self, field_name: &str) -> Result<Option<Expression>> {
        // TODO: What if the element size is dynamic (e.g. each buffer has another
        // buffer inside of it with a length field)
        let element_size = match self.element_type.get().size_of("")? {
            Some(v) => v,
            None => {
                return Ok(None);
            }
        }
        .scoped(field_name);

        let len = match self.proto.size_case() {
            BufferTypeProtoSizeCase::FixedLength(len) => Expression::Integer(*len as i64),
            BufferTypeProtoSizeCase::LengthFieldName(name) => Expression::Field(FieldExpression {
                field_path: vec![name.to_string()],
                attribute: Attribute::ValueOf,
            }),
            BufferTypeProtoSizeCase::EndTerminated(_) | BufferTypeProtoSizeCase::EndMarker(_) => {
                return Ok(None);
            }
            BufferTypeProtoSizeCase::Unknown => {
                return Err(err_msg("Unspecified buffer size"));
            }
        };

        Ok(Some(element_size.mul(len)))
    }
}
