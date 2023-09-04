use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};

use common::errors::*;
use common::line_builder::*;

use crate::expression::Expression;
use crate::expression::*;
use crate::proto::*;
use crate::types::*;

pub struct StructType<'a> {
    proto: &'a Struct,

    arguments: Vec<TypeReference<'a>>,

    /// NOTE: Don't iterate over this as it doesn't have a well defined order.
    fields: HashMap<&'a str, Field<'a>>,

    /// Map from the field name of a string to the list of fields which are
    /// parsed from that stream.
    stream_components: HashMap<&'a str, Vec<&'a str>>,
}

pub struct Field<'a> {
    pub proto: &'a FieldProto,
    typ: TypeReference<'a>,
}

impl<'a> StructType<'a> {
    pub fn create(proto: &'a Struct, resolver: &mut TypeResolver<'a>) -> Result<Self> {
        // TODO: Validate no duplicate arguments and not conflicting with arguments.

        let mut arguments = vec![];
        for arg in proto.argument() {
            arguments.push(resolver.resolve_type(
                arg.typ(),
                &TypeResolverContext {
                    // NOTE: Unused
                    endian: Endian::UNKNOWN,
                },
            )?);
        }

        // All fields in the struct indexed by name.
        let mut field_index: HashMap<&str, Field> = HashMap::new();

        // Fields which are used to indicate the size of another field so aren't
        // directly defined as a struct field.
        let mut derivated_fields: HashSet<&str> = HashSet::new();

        let mut stream_components: HashMap<&str, Vec<&str>> = HashMap::new();

        for field_proto in proto.field() {
            let field = Field {
                proto: field_proto,
                typ: resolver.resolve_type(
                    field_proto.typ(),
                    &TypeResolverContext {
                        endian: proto.endian(),
                    },
                )?,
            };

            if field_index.insert(&field_proto.name(), field).is_some() {
                return Err(format_err!("Duplicate field named: {}", field_proto.name()));
            }

            // TODO: Verify only used once and that the stream field exists.
            if !field_proto.stream().is_empty() {
                stream_components
                    .entry(field_proto.stream())
                    .or_default()
                    .push(field_proto.name());
            }
        }

        Ok(Self {
            proto,
            arguments,
            fields: field_index,
            stream_components,
        })
    }

    fn nice_field_name(name: &str) -> &str {
        if name == "type" {
            "typ"
        } else {
            name
        }
    }

    fn serialize_stream_value(
        &self,
        stream_field: &FieldProto,
        scope: &mut HashMap<&str, Symbol>,
        serialize_lines: &mut LineBuilder,
    ) -> Result<bool> {
        if !stream_field.value().is_empty() {
            return Err(err_msg("Stream should not value a normal value"));
        }

        let mut stream_buffer_lines = LineBuilder::new();
        stream_buffer_lines.add(format!("let mut {}_stream = vec![];", stream_field.name()));

        stream_buffer_lines.add("{");
        for component_name in self.stream_components[stream_field.name()].iter().cloned() {
            let field = &self.fields[component_name];

            let value_expr = match scope.get(component_name).and_then(|v| v.value.as_ref()) {
                Some(v) => v,
                None => return Ok(false),
            };

            let mut args = HashMap::new();
            for arg in field.proto.argument() {
                args.insert(
                    arg.name(),
                    match Expression::parse(arg.value())?.evaluate(&scope)? {
                        Some(v) => v,
                        None => return Ok(false),
                    },
                );
            }

            let ctx = TypeParserContext {
                stream: format!("(&mut {}_stream)", stream_field.name()),
                after_bytes: None,
                arguments: &args,
            };

            stream_buffer_lines.add(
                field
                    .typ
                    .get()
                    .serialize_bytes_expression(value_expr, &ctx)?,
            );
        }
        stream_buffer_lines.add("}");

        // TODO: Assert that stream fields don't have regular values.
        scope.get_mut(stream_field.name()).unwrap().value =
            Some(format!("{}_stream", stream_field.name()));

        serialize_lines.append(stream_buffer_lines);

        Ok(true)
    }

    // TODO: Can't have multiple end_terminated fields (where the first is in a
    // nested struct).
}

impl<'a> Type for StructType<'a> {
    fn compile_declaration(&self, lines: &mut LineBuilder) -> Result<()> {
        // For each first bit field in a sequence, this will store the number of bits
        // left until the last field in the sequence.
        let mut bit_field_spans: HashMap<&str, usize> = HashMap::new();

        // Validate that all bit fields align together to 8-bit boundaries
        // NOTE: We don't support interleaving non-bit fields with bit fields unless the
        // non-bit field starts at a 8-bit boundary.
        {
            let mut num_bits: Option<(&str, usize)> = None;
            for field in self.proto.field() {
                if !field.stream().is_empty() && field.bit_width() > 0 {
                    return Err(err_msg("Field in a stream can't have a bit_width"));
                }

                if field.bit_width() > 0 {
                    num_bits = Some(num_bits.map_or(
                        (field.name(), field.bit_width() as usize),
                        |(name, nbits)| (name, nbits + field.bit_width() as usize),
                    ));

                    // Only primitive fields can be used as bit fields.
                    if let TypeProtoTypeCase::Primitive(_) = field.typ().typ_case() {
                        // All good
                        // TODO: Must also validate that the given primitive
                        // type can fit the num bits.
                    } else if let TypeProtoTypeCase::Named(name) = field.typ().typ_case() {

                        // TODO: Look it up and hotpe that it is an enum with a
                        // primitive type (values can't be larger than the field
                        // size).
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
                    return Err(format_err!(
                        "Bits do not align to whole byte offsets. Total: {}",
                        nbits
                    ));
                }

                bit_field_spans.insert(first_field, nbits);
            }
        }

        // Number of bytes that follow each field in this struct.
        // Option<field_name, num_bytes>
        let mut end_terminated_marker = None;
        {
            let mut end_size = Expression::Integer(0);
            let mut end_bits = 0;
            let mut well_defined = true;

            // Set of all fields available before the current one being parsed.
            // aka this is all fields which can be referenced while parsing the next field
            // (assuming we parse from first to last field)
            let mut previous_fields = HashSet::new();
            for field in self.proto.field().iter() {
                previous_fields.insert(field.name());
            }

            for field in self.proto.field().iter().rev() {
                previous_fields.remove(&field.name());

                if let TypeProtoTypeCase::Buffer(b) = field.typ().typ_case() {
                    if let BufferTypeProtoSizeCase::EndTerminated(is_end_terminated) = b.size_case()
                    {
                        if !is_end_terminated {
                            return Err(err_msg("end_terminated field present but not true"));
                        }

                        if !well_defined {
                            return Err(err_msg(
                                "end_terminated buffer doesn't have a well defined number of bytes following it."));
                        }

                        let combined_size = end_size
                            .clone()
                            .add(Expression::Integer((end_bits / 8) as i64));

                        /*
                        if !combined_size
                            .referenced_field_names()
                            .is_subset(&previous_fields)
                        {
                            return Err(err_msg("Evaluating the size of an end_terminated field with look aheads is not supported"));
                        }
                        */

                        end_terminated_marker = Some((field.name(), combined_size));
                        well_defined = false;
                    }
                }

                // TODO: For the size, we need to verify that we size well defined based on only
                // already parsed values (assuming parsing from start to end)

                if field.bit_width() > 0 {
                    end_bits += field.bit_width() as usize;
                } else if let Some(byte_size) =
                    self.fields[field.name()].typ.get().size_of(field.name())?
                {
                    end_size = end_size.add(byte_size);
                } else {
                    well_defined = false;
                }
            }
        }

        let mut function_args = String::new();
        let mut function_arg_names = String::new();
        for (arg, arg_ty) in self.proto.argument().iter().zip(self.arguments.iter()) {
            // TODO: Conditionally take by reference depending on whether or not the type is
            // copyable.
            function_args.push_str(&format!(
                ", {}: &{}",
                arg.name(),
                arg_ty.get().type_expression()?
            ));

            function_arg_names.push_str(&format!(", {}", arg.name()));
        }

        ///////////////////////
        /// End of prep work
        ///////////////////////
        let mut struct_lines = LineBuilder::new();
        let mut default_values = LineBuilder::new();

        // Adding struct member delarations.
        for field in self.proto.field() {
            // Fields with constant/derived values don't need to be stored.
            if !field.value().is_empty() {
                continue;
            }

            // Don't need to store streams as they will be derived from other fields.
            if self.stream_components.contains_key(&field.name()) {
                continue;
            }

            let field_name = Self::nice_field_name(field.name());
            let field_ty = &self
                .fields
                .get(field.name())
                .ok_or_else(|| format_err!("Unknown field: {}", field.name()))?
                .typ;

            let mut typename = field_ty.get().type_expression()?;
            if !field.presence().is_empty() {
                typename = format!("Option<{}>", typename);
                default_values.add(format!("{}: None,\n", field_name));
            } else {
                default_values.add(format!(
                    "{}: {},\n",
                    field_name,
                    field_ty.get().default_value_expression()?
                ));
            }

            if !field.comment().is_empty() {
                struct_lines.add(format!("\t/// {}", field.comment()));
            }
            struct_lines.add(format!("\tpub {}: {},", field_name, typename));
        }

        let mut parser_lines = LineBuilder::new();
        let mut parser_field_values = LineBuilder::new();

        // Write the parser
        {
            // Map from field/argument names to the variable storing its value.
            let mut scope = HashMap::new();
            {
                for (arg, arg_type) in self.proto.argument().iter().zip(self.arguments.iter()) {
                    scope.insert(
                        arg.name(),
                        Symbol {
                            typ: arg_type.clone(),
                            value: Some(arg.name().to_string()),
                            size_of: None,
                            is_option: false,
                        },
                    );
                }

                for (field_name, field) in &self.fields {
                    scope.insert(
                        field_name,
                        Symbol {
                            typ: field.typ.clone(),
                            value: None,
                            size_of: None, // TODO: If a constant, we can evaluate this now.
                            is_option: !field.proto.presence().is_empty(),
                        },
                    );
                }
            }

            let mut bit_offset = 0;

            let mut pending_value_check = VecDeque::new();

            // Parse each field in the order of their appearance.
            for field in self.proto.field() {
                let field_typ = self.fields[field.name()].typ.get();

                let mut bit_slice = None;
                if field.bit_width() > 0 {
                    if let Some(span_width) = bit_field_spans.get(field.name()).cloned() {
                        // This is the first field in the bit slice, so we'll parse the full slice.
                        // TODO: We need to validate this this many bits actually exist in the
                        // input.
                        bit_offset = 0;
                        parser_lines.add(format!("let bit_input = &input[0..{}];", span_width / 8));
                        parser_lines.add("input = &input[bit_input.len()..];");
                    }

                    bit_slice = Some((bit_offset, (field.bit_width() as usize)));
                    bit_offset += field.bit_width() as usize;
                }

                let after_bytes = end_terminated_marker.clone().and_then(|(name, bytes)| {
                    if name == field.name() {
                        Some(bytes.evaluate(&scope).unwrap().unwrap())
                    } else {
                        None
                    }
                });

                // Need to implement  'presence'
                // - Evaluate the value of the field.
                // - Then if true, parse as normal, otherwise, don't and eval to None.

                let mut expr = {
                    let mut args = HashMap::new();
                    for arg in field.argument() {
                        args.insert(
                            arg.name(),
                            Expression::parse(arg.value())?
                                .evaluate(&scope)?
                                .ok_or_else(|| {
                                    format_err!(
                                        "While parsing field: {}, unable to evaluate: {}",
                                        field.name(),
                                        arg.value()
                                    )
                                })?,
                        );
                    }

                    let stream = {
                        if field.stream().is_empty() {
                            "input".to_string()
                        } else {
                            format!("{}_remaining", field.stream())
                        }
                    };

                    let ctx = TypeParserContext {
                        stream,
                        after_bytes,
                        arguments: &args,
                    };

                    if let Some((bit_offset, bit_width)) = bit_slice {
                        field_typ.parse_bits_expression(bit_offset, bit_width)?
                    } else {
                        field_typ.parse_bytes_expression(&ctx)?
                    }
                };
                if !field.presence().is_empty() {
                    // TODO: Parse the presence field to validate that it actually is a valid field
                    // reference rather than just arbitrary code.
                    expr = format!(
                        r#"{{
                        let present = {};
                        if present {{
                            Some({})
                        }} else {{
                            None
                        }}
                        }}"#,
                        Expression::parse(field.presence())?
                            .evaluate(&scope)?
                            .unwrap(),
                        expr
                    );
                }

                let var_name = format!("{}_value", Self::nice_field_name(field.name()));
                parser_lines.add(format!(
                    "
                    let {name}_before_len = input.len();
                    let {var_name} = {expr};
                    let {name}_after_len = input.len();
                    let {name}_size_of = {name}_before_len - {name}_after_len;
                    
                ",
                    name = field.name(),
                    var_name = var_name,
                    expr = expr
                ));

                if self.stream_components.contains_key(&field.name()) {
                    parser_lines.add(format!(
                        "let mut {}_remaining: &[u8] = &{}[..];",
                        field.name(),
                        var_name
                    ));
                }

                // When parsing the last field in a stream, verify that all bytes in the stream
                // have been consumed.
                if !field.stream().is_empty()
                    && *self.stream_components[field.stream()].last().unwrap() == field.name()
                {
                    parser_lines.add(format!(
                        r#"
                        if !{}_remaining.is_empty() {{
                            return Err(err_msg("Extra bytes at end of stream"));
                        }}
                        "#,
                        field.stream(),
                    ));
                }

                // The value has now been parsed so we can reference it in expressions
                scope.get_mut(field.name()).unwrap().value = Some(var_name.clone());
                scope.get_mut(field.name()).unwrap().size_of =
                    Some(format!("{name}_size_of", name = field.name()));

                if !field.value().is_empty() {
                    pending_value_check.push_back(field.name());
                }

                // Validate the value of each field as soon as we have parsed enough information
                // to do so.
                while !pending_value_check.is_empty() {
                    let field_name = pending_value_check[0];
                    let field = &self.fields[field_name];

                    let value = match Expression::parse(field.proto.value())?.evaluate(&scope)? {
                        Some(v) => v,
                        None => break,
                    };

                    parser_lines.add(format!(
                        r#"
                        {{
                            let expected_value = {} as {};
                            if expected_value != {}_value{} {{
                                return Err(err_msg("Wrong field value"));
                            }}
                        }}
                        "#,
                        value,
                        field.typ.get().type_expression()?,
                        Self::nice_field_name(field_name),
                        // TODO: Have a better solution than this.
                        if field.proto.presence().is_empty() {
                            ""
                        } else {
                            ".unwrap_or(0)"
                        }
                    ));

                    pending_value_check.pop_front();
                }

                if field.value().is_empty() && !self.stream_components.contains_key(field.name()) {
                    parser_field_values.add(format!(
                        "\t\t\t{}: {},",
                        Self::nice_field_name(field.name()),
                        var_name
                    ));
                }
            }

            if !pending_value_check.is_empty() {
                return Err(format_err!(
                    "Unable to evaluate value for fields: {:?}",
                    pending_value_check
                ));
            }
        }

        let mut field_accessors = LineBuilder::new();
        let mut serialize_lines = LineBuilder::new();

        // Write the serializer
        {
            // Map from field/argument names to the variable storing its value.
            let mut scope = HashMap::new();
            {
                // Add all arguments.
                for (arg, arg_type) in self.proto.argument().iter().zip(self.arguments.iter()) {
                    scope.insert(
                        arg.name(),
                        Symbol {
                            typ: arg_type.clone(),
                            value: Some(arg.name().to_string()),
                            size_of: None,
                            is_option: false,
                        },
                    );
                }

                // Initialize scope with all plain fields (those that have their values simply
                // stored as fields in the struct).
                for (field_name, field) in &self.fields {
                    let has_stored_value = field.proto.value().is_empty()
                        && !self.stream_components.contains_key(field_name);

                    let value = if has_stored_value {
                        Some(format!("self.{}", Self::nice_field_name(field_name)))
                    } else {
                        None
                    };

                    scope.insert(
                        field_name,
                        Symbol {
                            typ: field.typ.clone(),
                            value,
                            size_of: None, // TODO: If a constant, we can evaluate this now.
                            is_option: !field.proto.presence().is_empty(),
                        },
                    );
                }

                // Add field accessors for derived fields.
                // We need to run this multiple times as derived fields may reference each
                // other.
                // This would also be a good place to derive stream values.
                let mut made_progress = true;
                while made_progress {
                    made_progress = false;

                    for (field_name, field) in &self.fields {
                        if scope[field_name].value.is_some() {
                            continue;
                        }

                        if self.stream_components.contains_key(field_name) {
                            made_progress |= self.serialize_stream_value(
                                &field.proto,
                                &mut scope,
                                &mut serialize_lines,
                            )?;

                            continue;
                        }

                        let expr = match Expression::parse(field.proto.value())?.evaluate(&scope)? {
                            Some(v) => v,
                            None => continue,
                        };

                        made_progress = true;

                        let field_typ = self.fields[field_name].typ.get();

                        scope.get_mut(field_name).unwrap().value = Some(format!(
                            "({expr} as {ty})",
                            ty = field_typ.type_expression()?,
                            expr = expr,
                        ));

                        // TODO: Add field accessors only if we can cheaply
                        // calculate it (doesn't depend on any stream values).
                        /*
                        field_accessors.add(format!(
                            "\tpub fn {method}(&self) -> {ty} {{ {expr} as {ty} }}",
                            method = Self::nice_field_name(field_name),
                            ty = field_typ.type_expression()?,
                            expr = expr,
                            // field_typ.value_expression(field.constant_value())?
                        ));

                        scope.get_mut(field_name).unwrap().value =
                            Some(format!("self.{}()", Self::nice_field_name(field_name)));
                        */
                    }
                }
            }

            // At this point, we should have a value for all fields.

            let mut bit_offset = 0;

            for field in self.proto.field() {
                // Skip fields in streams which we have handled earlier.
                if !field.stream().is_empty() {
                    continue;
                }

                // Update it in case it changed.
                let value_expr = scope[field.name()]
                    .value
                    .clone()
                    .ok_or_else(|| format_err!("Missing value for '{}'", field.name()))?;

                let output_buffer = "out".to_string();

                let mut bit_slice = None;
                if field.bit_width() > 0 {
                    if let Some(span_width) = bit_field_spans.get(field.name()).cloned() {
                        // This is the first field in the bit slice, so we'll allocate some
                        // space for the entire slice and
                        // then we'll use it.
                        bit_offset = 0;

                        serialize_lines.add(format!(
                            r#"
                            let bit_output = {{
                                let start = {output_buffer}.len();
                                {output_buffer}.resize(start + {idx}, 0);
                                &mut {output_buffer}[start..(start + {idx})]
                            }};
                            "#,
                            output_buffer = output_buffer,
                            idx = span_width / 8
                        ));
                    }

                    bit_slice = Some((bit_offset, (field.bit_width() as usize)));
                    bit_offset += field.bit_width() as usize;
                }

                let mut args = HashMap::new();
                for arg in field.argument() {
                    args.insert(
                        arg.name(),
                        // TODO: May need to defer evluation of the field due to this.
                        Expression::parse(arg.value())?
                            .evaluate(&scope)?
                            .ok_or_else(|| {
                                format_err!(
                                    "Failed to evaluate arg {}: {}",
                                    arg.name(),
                                    arg.value()
                                )
                            })?,
                    );
                }

                let ctx = TypeParserContext {
                    stream: output_buffer.clone(),
                    after_bytes: None,
                    arguments: &args,
                };

                let get_serializer = |value_expr: &str| {
                    if let Some((bit_offset, bit_width)) = bit_slice {
                        self.fields[field.name()]
                            .typ
                            .get()
                            .serialize_bits_expression(value_expr, bit_offset, bit_width)
                    } else {
                        self.fields[field.name()]
                            .typ
                            .get()
                            .serialize_bytes_expression(value_expr, &ctx)
                    }
                };

                let line = {
                    // TODO: 'presence' fields should be considered to be derived so we can
                    // skip adding it to the struct (but we can keep the runtime check which
                    // will hopefully compile away).
                    // TODO: Have a more robust/consistent way of telling if the value is stored as
                    // an Option<>.
                    if !field.presence().is_empty() && field.value().is_empty() {
                        format!(
                            r#"{{
                            let present = {};
                            let value = &{};
                            if value.is_some() != present {{
                                return Err(err_msg("Mismatch between"));
                            }}

                            if let Some(v) = value {{
                                {}
                            }}
                            }}"#,
                            Expression::parse(field.presence())?
                                .evaluate(&scope)?
                                .unwrap(),
                            value_expr,
                            get_serializer("v")?
                        )
                    } else {
                        get_serializer(&value_expr)?
                    }
                };

                serialize_lines.add(format!("\t\t{}", line));
            }
        }

        {
            // Consider adding a size_of
            let struct_size = self.size_of("")?;

            // TODO: Verify that there are no fields named 'size_of'
            if let Some(fixed_size) = struct_size.clone().and_then(|s| s.to_constant()) {
                field_accessors.add(format!(
                    "\tpub const fn size_of() -> usize {{ {} }}",
                    fixed_size
                ));
                field_accessors.nl();
            }

            if let Some(1) = struct_size.and_then(|s| s.to_constant()) {
                // TODO: Optimize this further?
                field_accessors.add(format!(
                    r#"
                    pub fn to_u8(&self{}) -> Result<u8> {{
                        let mut data = vec![];
                        data.reserve_exact(1);
                        self.serialize(&mut data{})?;
                        assert_eq!(data.len(), 1);
                        Ok(data[0])
                    }}
                "#,
                    function_args, function_arg_names
                ));
            }
        }

        // TODO: Consider in some cases using repr(C).
        lines.add(format!(
            r#"
            #[derive(Debug, PartialEq, Clone)]
            pub struct {name} {{
                {struct_lines}
            }}

            impl {name} {{
                pub fn parse_complete(input: &[u8]{function_args}) -> Result<Self> {{
                    let (v, _) = ::parsing::complete(move |i| Self::parse(i{function_arg_names}))(input)?;
                    Ok(v)
                }} 

                pub fn parse<'a>(mut input: &'a [u8]{function_args}) -> Result<(Self, &'a [u8])> {{
                    {parser_lines}

                    Ok((Self {{
                        {parser_field_values}
                    }}, input))
                }}

                pub fn serialize(&self, out: &mut Vec<u8>{function_args}) -> Result<()> {{
                    {serialize_lines}

                    Ok(())
                }}

                {field_accessors}
            }}

            impl Default for {name} {{
                fn default() -> Self {{
                    Self {{
                        {default_values}
                    }}
                }}
            }}

            "#,
            name = self.proto.name(),
            function_args = function_args,
            function_arg_names = function_arg_names,
            struct_lines = struct_lines.to_string(),
            parser_lines = parser_lines.to_string(),
            parser_field_values = parser_field_values.to_string(),
            default_values = default_values.to_string(),
            serialize_lines = serialize_lines.to_string(),
            field_accessors = field_accessors.to_string(),
        ));

        Ok(())
    }

    fn type_expression(&self) -> Result<String> {
        Ok(self.proto.name().to_string())
    }

    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String> {
        let mut args = String::new();
        for arg in self.proto.argument() {
            let val = context.arguments.get(arg.name()).ok_or_else(|| {
                format_err!(
                    "Argument '{}' not provided to '{}'",
                    arg.name(),
                    self.proto.name()
                )
            })?;
            args = format!(", &{}", val);
        }

        Ok(format!(
            "parse_next!(input, |i| {}::parse(i{}))",
            self.proto.name(),
            args
        ))
    }

    fn serialize_bytes_expression(
        &self,
        value: &str,
        context: &TypeParserContext,
    ) -> Result<String> {
        let mut args = String::new();
        for arg in self.proto.argument() {
            let val = context.arguments.get(arg.name()).ok_or_else(|| {
                format_err!(
                    "Argument '{}' not provided to '{}'",
                    arg.name(),
                    self.proto.name()
                )
            })?;
            args = format!(", &{}", val);
        }

        Ok(format!("{}.serialize({}{})?;", value, context.stream, args))
    }

    fn size_of(&self, field_name: &str) -> Result<Option<Expression>> {
        // TODO: Ideally we should cache these.

        let mut total_size = Expression::Integer(0);
        let mut bits = 0;

        for (_, field) in &self.fields {
            if field.proto.bit_width() > 0 {
                bits += field.proto.bit_width() as usize;
            } else {
                let field_size = {
                    // TODO: Handle re-writing of argument names based on the original values.
                    if let Some(v) = field.typ.get().size_of(field.proto.name())? {
                        v
                    } else {
                        return Ok(None);
                    }
                }
                .scoped(field_name);

                total_size = total_size.add(field_size);
            }
        }

        total_size = total_size.add(Expression::Integer((bits / 8) as i64));

        Ok(Some(total_size))
    }
}
