use std::collections::HashMap;

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
            | BufferTypeProtoSizeCase::Length(_) // TODO: Might be a constant expression.
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
            | BufferTypeProtoSizeCase::Length(_) // TODO: The expression might be a constant
            | BufferTypeProtoSizeCase::EndTerminated(_)
            | BufferTypeProtoSizeCase::EndMarker(_) => {
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

        let mut lines = LineBuilder::new();
        lines.add("{");

        match self.proto.size_case() {
            // TODO: Some types like large primitive slices can be optimized.
            BufferTypeProtoSizeCase::FixedLength(len) => {
                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    // TODO: Ensure that we always take exact a slice (and not Bytes as that
                    // is an expensive copy)!
                    lines.add(format!(
                        r#"
                        let mut buf = [{element_ty}::default(); {len}];
                        let n = buf.len();
                        buf.copy_from_slice(parse_next!({input}, ::parsing::take_exact(n)));
                        buf
                        "#,
                        input = context.stream,
                        element_ty = self.element_type.get().type_expression()?,
                        len = len
                    ));
                } else {
                    let element_parser = self.element_type.get().parse_bytes_expression(context)?;

                    // TODO: Need to generally verify that no arguments use this name either.
                    // It would be easier if we wrap the logic in a function.
                    assert!(context.stream != "buf");

                    lines.add(format!(
                        r#"
                        let mut buf: [core::mem::MaybeUninit<{element_ty}>; {len}] = core::mem::MaybeUninit::uninit_array();

                        for i in 0..buf.len() {{
                            buf[i].write({element_parser});
                        }}

                        unsafe {{ core::mem::MaybeUninit::array_assume_init(buf) }}
                        "#,
                        element_ty = self.element_type.get().type_expression()?,
                        len = len,
                        element_parser = element_parser
                    ));
                }
            }
            BufferTypeProtoSizeCase::Length(expr) => {
                let mut scope = HashMap::new();
                for (arg_name, arg_expr) in context.arguments {
                    scope.insert(
                        *arg_name,
                        Symbol {
                            typ: self.element_type.clone(), // TODO: Configure to the correct type.
                            value: Some(arg_expr.clone()),
                            size_of: None,
                        },
                    );
                }

                let len_expr = Expression::parse(expr)?.evaluate(&scope)?.unwrap();

                lines.add("let mut buf = vec![];");
                assert!(context.stream != "buf");

                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    lines.add(format!(
                        "\tbuf.extend_from_slice(parse_next!({}, ::parsing::take_exact({} as usize)));",
                        context.stream,
                        len_expr
                    ));
                } else {
                    let element_parser = self.element_type.get().parse_bytes_expression(context)?;

                    lines.add(format!("\tbuf.reserve({} as usize);", len_expr));
                    lines.add(format!("\tfor _ in 0..({} as usize) {{", len_expr));
                    lines.add(format!("\t\tbuf.push({});", element_parser));
                    lines.add("\t}");
                }

                lines.add("\tbuf");
            }

            BufferTypeProtoSizeCase::LengthFieldName(name) => todo!(), // Deprecated
            BufferTypeProtoSizeCase::EndTerminated(_) => {
                let after_count = context
                    .after_bytes
                    .as_ref()
                    .ok_or(err_msg("end_terminated buffer not validated"))?;

                lines.add("\tlet mut buf = vec![];");

                // TODO: Fix the remaining_bytes value used.
                lines.add(format!(
                    r#"
                    let length = {}.len().checked_sub({})
                        .ok_or_else(|| ::parsing::incomplete_error(0))?;"#,
                    context.stream, after_count
                ));

                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    lines.add(format!(
                        "buf.extend_from_slice(parse_next!({input}, ::parsing::take_exact(length)));",
                        input = context.stream
                    ));
                } else {
                    // Here we need to change the internal parser.

                    let element_parser =
                        self.element_type
                            .get()
                            .parse_bytes_expression(&TypeParserContext {
                                stream: "input".to_string(),
                                after_bytes: None,
                                arguments: &context.arguments,
                            })?;

                    lines.add(format!(
                        r#"
                        let mut input = parse_next!({input}, ::parsing::take_exact(length));
                        while !input.is_empty() {{
                            buf.push({element_parser});
                        }}
                        "#,
                        input = context.stream,
                        element_parser = element_parser
                    ));
                }

                lines.add("\tbuf");
            }
            BufferTypeProtoSizeCase::EndMarker(marker) => {
                lines.add("\tlet mut buf = vec![];");

                lines.add(format!(
                    "const MARKER: &'static [u8] = &{:?};",
                    marker.as_ref()
                ));
                lines.add(format!(r#"
                    let mut input = parse_next!({input}, |i| ::parsing::search::parse_pattern_terminated_bytes(i, MARKER));
                "#, input = context.stream));

                // TODO: We may be able to reserve memory if we know the size of each element.

                // TODO: Deduplicate this.
                if let TypeProtoTypeCase::Primitive(PrimitiveTypeProto::U8) =
                    self.proto.element_type().type_case()
                {
                    lines.add("\tbuf.extend_from_slice(input);");
                } else {
                    let element_parser =
                        self.element_type
                            .get()
                            .parse_bytes_expression(&TypeParserContext {
                                stream: "input".to_string(),
                                after_bytes: None,
                                arguments: &context.arguments,
                            })?;

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
        context: &TypeParserContext,
    ) -> Result<String> {
        let mut lines = LineBuilder::new();

        lines.add("{");

        // TODO: Also implement the 'Length' case like this.
        if let BufferTypeProtoSizeCase::Length(expr) = self.proto.size_case() {
            let mut scope = HashMap::new();
            for (arg_name, arg_expr) in context.arguments {
                scope.insert(
                    *arg_name,
                    Symbol {
                        typ: self.element_type.clone(), // TODO: Configure to the correct type.
                        value: Some(arg_expr.clone()),
                        size_of: None,
                    },
                );
            }

            let len_value = Expression::parse(expr)?.evaluate(&scope)?.unwrap();

            lines.add(format!(
                r#"
                if {} as usize != ({}).len() {{
                    return Err(err_msg("Data length does not match length field"));
                }}
            "#,
                len_value, value
            ));
        }

        lines.add(format!("let start_i = {}.len();", context.stream));

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
                    .serialize_bytes_expression("item", context)?
            ));
            lines.add("}");
        }

        if let BufferTypeProtoSizeCase::EndMarker(marker) = self.proto.size_case() {
            lines.add(format!(
                r#"
                let end_i = {output_buffer}.len();
                const MARKER: &'static [u8] = &{marker:?};
                {output_buffer}.extend_from_slice(MARKER);

                if ::parsing::search::find_byte_pattern(&{output_buffer}[start_i..], MARKER) != Some(end_i - start_i)  {{
                    return Err(err_msg("Data contains end marker"));
                }}
                "#,
                output_buffer = context.stream,
                marker = marker.as_ref(),
            ));
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
            BufferTypeProtoSizeCase::Length(expr) => Expression::parse(expr)?,
            BufferTypeProtoSizeCase::LengthFieldName(name) => panic!(), // Deprecated
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
