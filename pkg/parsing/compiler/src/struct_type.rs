use std::collections::{HashMap, HashSet};

use common::errors::*;
use common::line_builder::*;

use crate::proto::*;
use crate::size::*;
use crate::types::*;

pub struct StructType<'a> {
    proto: &'a Struct,

    arguments: Vec<TypeReference<'a>>,

    /// NOTE: Don't iterate over this as it doesn't have a well defined order.
    fields: HashMap<&'a str, Field<'a>>,

    derivated_fields: HashSet<&'a str>,
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

            let used_names = Self::referenced_field_names(field_proto.typ());

            for name in &used_names {
                if !field_index.contains_key(name) {
                    // TODO: If a length field is used in multiple different fields, then we need to
                    // do validation at serialization time that sizes are correct.
                    // TODO: Eventually support reading fields from the back of a struct in some
                    // cases.
                    return Err(format_err!("Field referenced before parsed: {}", name));
                }
            }

            derivated_fields.extend(&used_names);
        }

        Ok(Self {
            proto,
            arguments,
            fields: field_index,
            derivated_fields,
        })
    }

    fn referenced_field_names(typ: &'a TypeProto) -> HashSet<&'a str> {
        let mut out = HashSet::new();

        fn recurse<'a>(t: &'a TypeProto, out: &mut HashSet<&'a str>) {
            if let TypeProtoTypeCase::Buffer(buf) = t.type_case() {
                if let BufferTypeProtoSizeCase::LengthFieldName(name) = buf.size_case() {
                    if let Some((field, _)) = name.as_str().split_once('.') {
                        out.insert(field);
                    } else {
                        out.insert(&name);
                    }
                }

                recurse(buf.element_type(), out);
            }
        }

        recurse(typ, &mut out);
        out
    }

    fn nice_field_name(name: &str) -> &str {
        if name == "type" {
            "typ"
        } else {
            name
        }
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
                if field.bit_width() > 0 {
                    num_bits = Some(num_bits.map_or(
                        (field.name(), field.bit_width() as usize),
                        |(name, nbits)| (name, nbits + field.bit_width() as usize),
                    ));

                    // Only primitive fields can be used as bit fields.
                    if let TypeProtoTypeCase::Primitive(_) = field.typ().type_case() {
                        // All good
                        // TODO: Must also validate that the given primitive
                        // type can fit the num bits.
                    } else if let TypeProtoTypeCase::Named(name) = field.typ().type_case() {

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

        // Option<field_name, num_bytes>
        let mut end_terminated_marker = None;
        {
            let mut end_size = SizeExpression::Constant(0);
            let mut end_bits = 0;
            let mut well_defined = true;

            // Set of all fields available before the current one being parsed.
            // aka this is all fields which can be referenced
            let mut previous_fields = HashSet::new();
            for field in self.proto.field().iter() {
                previous_fields.insert(field.name());
            }

            for field in self.proto.field().iter().rev() {
                previous_fields.remove(&field.name());

                if let TypeProtoTypeCase::Buffer(b) = field.typ().type_case() {
                    if let BufferTypeProtoSizeCase::EndTerminated(is_end_terminated) = b.size_case()
                    {
                        if !is_end_terminated {
                            return Err(err_msg("end_terminated field present but not true"));
                        }

                        if !well_defined {
                            return Err(err_msg(
                                "end_terminated buffer doesn't have a well defined number of bytes following it."));
                        }

                        let combined_size =
                            end_size.clone().add(SizeExpression::Constant(end_bits / 8));

                        if !combined_size
                            .referenced_field_names()
                            .is_subset(&previous_fields)
                        {
                            return Err(err_msg("Evaluating the size of an end_terminated field with look aheads is not supported"));
                        }

                        end_terminated_marker = Some((field.name(), combined_size));
                        well_defined = false;
                    }
                }

                // TODO: For the size, we need to verify that we size well defined based on only
                // already parsed values (assuming parsing from start to end)

                if field.bit_width() > 0 {
                    end_bits += field.bit_width() as usize;
                } else if let Some(byte_size) =
                    self.fields[field.name()].typ.get().sizeof(field.name())?
                {
                    end_size = end_size.add(byte_size);
                } else {
                    well_defined = false;
                }
            }
        }

        // TODO: Consider using packed memory?
        lines.add("#[derive(Debug, PartialEq, Clone)]");
        lines.add(format!("pub struct {} {{", self.proto.name()));

        // Adding struct member delarations.
        let mut default_values = LineBuilder::new();
        for field in self.proto.field() {
            if self.derivated_fields.contains(field.name()) {
                if let TypeProtoTypeCase::Primitive(_) = field.typ().type_case() {
                    // All is good.
                } else {
                    // TODO: We should be more specific. Only allow unsigned integer types?
                    return Err(err_msg("Expected length fields to have scaler types"));
                }

                continue;
            }

            let field_name = Self::nice_field_name(field.name());
            let field_ty = &self.fields[field_name].typ;

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
                lines.add(format!("\t/// {}", field.comment()));
            }
            lines.add(format!("\tpub {}: {},", field_name, typename));
        }

        lines.add("}");
        lines.nl();

        lines.add(format!("impl Default for {} {{", self.proto.name()));
        lines.add("fn default() -> Self {");
        lines.add("Self {");
        lines.append(default_values);
        lines.add("}");
        lines.add("}");
        lines.add("}");
        lines.nl();

        lines.add(format!("impl {} {{", self.proto.name()));

        // Consider adding a size_of
        let struct_size = self.sizeof("")?;

        // TODO: Verify that there are no fields named 'size_of'
        if let Some(fixed_size) = struct_size.clone().and_then(|s| s.to_constant()) {
            lines.add(format!(
                "\tpub const fn size_of() -> usize {{ {} }}",
                fixed_size
            ));
            lines.nl();
        }

        // Add accessors for derived fields,
        //
        // TODO: If a length field is referenced multiple times, then we need to verify
        // that all vectors have consistent length. TODO: Also if the size is
        // used as an inner dimension of a buffer, then we can't determin
        for field in self.proto.field() {
            if let TypeProtoTypeCase::Buffer(buf) = field.typ().type_case() {
                if let BufferTypeProtoSizeCase::LengthFieldName(name) = buf.size_case() {
                    // TODO: Challenge here is that we must ensure that the size fits within the
                    // limits of the type (no overflows when serializing).
                    let size_type = self.fields[name.as_str()].typ.get().type_expression()?;
                    lines.add(format!(
                        "\tpub fn {}(&self) -> {} {{ self.{}.len() as {} }}",
                        name,
                        size_type,
                        Self::nice_field_name(field.name()),
                        size_type
                    ));
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

        lines.add(format!(
            r#"
            pub fn parse_complete(input: &[u8]{}) -> Result<Self> {{
                let (v, _) = ::parsing::complete(move |i| Self::parse(i{}))(input)?;
                Ok(v)
            }} 
        "#,
            function_args, function_arg_names
        ));

        // Also need to support parsing from Bytes to have fewer copies.
        lines.add(format!(
            "\tpub fn parse<'a>(mut input: &'a [u8]{}) -> Result<(Self, &'a [u8])> {{",
            function_args
        ));
        {
            let mut bit_offset = 0;

            for field in self.proto.field() {
                let mut bit_slice = None;
                if field.bit_width() > 0 {
                    if let Some(span_width) = bit_field_spans.get(field.name()).cloned() {
                        // This is the first field in the bit slice, so we'll parse the full slice.
                        // TODO: We need to validate this this many bits actually exist in the
                        // input.
                        bit_offset = 0;
                        lines.add(format!("let bit_input = &input[0..{}];", span_width / 8));
                        lines.add("input = &input[bit_input.len()..];");
                    }

                    bit_slice = Some((bit_offset, (field.bit_width() as usize)));
                    bit_offset += field.bit_width() as usize;
                }

                let after_bytes = end_terminated_marker.clone().and_then(|(name, bytes)| {
                    if name == field.name() {
                        Some(bytes.compile(&self.fields))
                    } else {
                        None
                    }
                });

                // Need to implement  'presence'
                // - Evaluate the value of the field.
                // - Then if true, parse as normal, otherwise, don't and eval to None.

                let mut expr = {
                    let ctx = TypeParserContext {
                        after_bytes,
                        scope: &self.fields,
                    };

                    if let Some((bit_offset, bit_width)) = bit_slice {
                        self.fields[field.name()]
                            .typ
                            .get()
                            .parse_bits_expression(bit_offset, bit_width)?
                    } else {
                        self.fields[field.name()]
                            .typ
                            .get()
                            .parse_bytes_expression(&ctx)?
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
                        field.presence(),
                        expr
                    );
                }

                lines.add(format!(
                    "\t\tlet {}_value = {};",
                    Self::nice_field_name(field.name()),
                    expr
                ));
            }
            lines.nl();

            lines.add(format!("\t\tOk(({} {{", self.proto.name()));
            for field in self.proto.field() {
                if self.derivated_fields.contains(field.name()) {
                    continue;
                }

                lines.add(format!(
                    "\t\t\t{}: {}_value,",
                    Self::nice_field_name(field.name()),
                    Self::nice_field_name(field.name())
                ));
            }
            lines.add("\t\t}, input))");
        }
        lines.add("\t}");
        lines.nl();

        if let Some(1) = struct_size.and_then(|s| s.to_constant()) {
            // TODO: Optimize this further?
            lines.add(format!(
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

        lines.add(format!(
            "\tpub fn serialize(&self, out: &mut Vec<u8>{}) -> Result<()> {{",
            function_args
        ));
        {
            // TODO: Need to support lots of exotic derived fields.

            let mut bit_offset = 0;

            for field in self.proto.field() {
                let mut bit_slice = None;
                if field.bit_width() > 0 {
                    if let Some(span_width) = bit_field_spans.get(field.name()).cloned() {
                        // This is the first field in the bit slice, so we'll allocate some space
                        // for the entire slice and then we'll use it.
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

                let value_expr = {
                    if self.derivated_fields.contains(field.name()) {
                        format!("self.{}()", Self::nice_field_name(field.name()))
                    } else {
                        format!("self.{}", Self::nice_field_name(field.name()))
                    }
                };

                let get_parser = |value_expr: &str| {
                    if let Some((bit_offset, bit_width)) = bit_slice {
                        self.fields[field.name()]
                            .typ
                            .get()
                            .serialize_bits_expression(value_expr, bit_offset, bit_width)
                    } else {
                        self.fields[field.name()]
                            .typ
                            .get()
                            .serialize_bytes_expression(value_expr)
                    }
                };

                let line = {
                    // TODO: 'presence' fields should be considered to be derived so we can
                    // skip adding it to the struct (but we can keep the runtime check which will
                    // hopefully compile away).
                    if !field.presence().is_empty() && !self.derivated_fields.contains(field.name())
                    {
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
                            field.presence(),
                            value_expr,
                            get_parser("v")?
                        )
                    } else {
                        get_parser(&value_expr)?
                    }
                };

                lines.add(format!("\t\t{}", line));
            }

            lines.add("\t\tOk(())");
        }
        lines.add("\t}");

        lines.add("}");

        // Now we need a parse and serialize routine.

        Ok(())
    }

    fn type_expression(&self) -> Result<String> {
        Ok(self.proto.name().to_string())
    }

    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String> {
        Ok(format!("parse_next!(input, {}::parse)", self.proto.name()))
    }

    fn serialize_bytes_expression(&self, value: &str) -> Result<String> {
        Ok(format!("{}.serialize(out)?;", value))
    }

    fn sizeof(&self, field_name: &str) -> Result<Option<SizeExpression>> {
        // TODO: Ideally we should cache these.

        let mut total_size = SizeExpression::Constant(0);
        let mut bits = 0;

        for (_, field) in &self.fields {
            if field.proto.bit_width() > 0 {
                bits += field.proto.bit_width() as usize;
            } else {
                let field_size = {
                    if let Some(v) = field.typ.get().sizeof(field.proto.name())? {
                        v
                    } else {
                        return Ok(None);
                    }
                }
                .scoped(field_name);

                total_size = total_size.add(field_size);
            }
        }

        total_size = total_size.add(SizeExpression::Constant(bits / 8));

        Ok(Some(total_size))
    }
}
