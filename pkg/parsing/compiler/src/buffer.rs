use common::errors::*;
use common::line_builder::*;

use crate::proto::*;
use crate::size::SizeExpression;
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
            | BufferTypeProtoSizeCase::EndTerminated(_) => {
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
            | BufferTypeProtoSizeCase::EndTerminated(_) => {
                format!("vec![]")
            }
        })
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
        // self.compile_parse_type(&buf.element_type(), endian, None, None, scope)?;

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
                    let field = context.scope.get(name.as_str()).unwrap();
                    if !field.proto.presence().is_empty() {
                        format!("{}_value.unwrap_or(0) as usize", name)
                    } else {
                        format!("{}_value as usize", name)
                    }
                };

                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    lines.add(format!(
                        "\tbuf.extend_from_slice(parse_next!(input, ::parsing::take_exact({})));",
                        len_expr
                    ));
                } else {
                    lines.add(format!("\tbuf.reserve({});", len_expr));
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
            BufferTypeProtoSizeCase::Unknown => {
                panic!();
            }
        }

        lines.add("}");
        lines.indent();
        lines.indent();

        Ok(lines.to_string())
    }

    fn serialize_bytes_expression(&self, value: &str) -> Result<String> {
        // Optimized case for [u8]
        if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
            self.proto.element_type().type_case()
        {
            return Ok(format!("out.extend_from_slice(&{});", value));
        }

        let mut lines = LineBuilder::new();
        lines.add(format!("for item in &{} {{", value));
        lines.add(format!(
            "\t{}",
            self.element_type.get().serialize_bytes_expression("item")?
        ));
        lines.add("}");
        Ok(lines.to_string())
    }

    // TODO: This will have a well defined length if we can reference a field
    // (unfortunately this is harder for nested fields).
    fn sizeof(&self, field_name: &str) -> Result<Option<crate::size::SizeExpression>> {
        // TODO: What if the element size is dynamic (e.g. each buffer has another
        // buffer inside of it with a length field)
        let element_size = match self.element_type.get().sizeof("")? {
            Some(v) => v,
            None => {
                return Ok(None);
            }
        }
        .scoped(field_name);

        let len = match self.proto.size_case() {
            BufferTypeProtoSizeCase::FixedLength(len) => SizeExpression::Constant(*len as usize),
            BufferTypeProtoSizeCase::LengthFieldName(name) => {
                SizeExpression::FieldLength(vec![name.to_string()])
            }
            BufferTypeProtoSizeCase::EndTerminated(_) => {
                return Ok(None);
            }
            BufferTypeProtoSizeCase::Unknown => {
                return Err(err_msg("Unspecified buffer size"));
            }
        };

        Ok(Some(element_size.mul(len)))
    }
}
