use std::collections::HashMap;

use common::errors::*;
use common::line_builder::*;

use crate::buffer::BufferType;
use crate::expression::evaluate_expression;
use crate::proto::*;
use crate::size::SizeExpression;
use crate::types::*;

pub struct UnionType<'a> {
    proto: &'a UnionTypeProto,
    argument_types: HashMap<&'a str, TypeReference<'a>>,
    case_types: HashMap<&'a str, TypeReference<'a>>,
    has_default: bool,
}

impl<'a> UnionType<'a> {
    pub fn create(proto: &'a UnionTypeProto, resolver: &mut TypeResolver<'a>) -> Result<Self> {
        let context = TypeResolverContext {
            endian: proto.endian(),
        };

        let mut argument_types = HashMap::default();
        for arg in proto.argument() {
            let t = resolver.resolve_type(arg.typ(), &context)?;
            if argument_types.insert(arg.name(), t).is_some() {
                return Err(format_err!("Duplicate argument named: {}", arg.name()));
            }
        }

        let mut case_types = HashMap::new();
        let mut has_default = false;
        for (i, case) in proto.case().iter().enumerate() {
            let t = resolver.resolve_type(case.typ(), &context)?;
            if case_types.insert(case.name(), t).is_some() {
                return Err(format_err!("Duplicate case named: {}", case.name()));
            }

            if case.is_default() && i != proto.case_len() - 1 {
                return Err(err_msg("Expected the default case to be last."));
            }
        }

        Ok(Self {
            proto,
            argument_types,
            case_types,
            has_default,
        })
    }
}

impl<'a> Type for UnionType<'a> {
    fn compile_declaration(&self, out: &mut LineBuilder) -> Result<()> {
        let mut function_args = String::new();
        let mut function_arg_names = String::new();
        let mut scope = HashMap::default();
        for arg in self.proto.argument().iter() {
            let arg_ty = self.argument_types.get(arg.name()).unwrap();

            // TODO: Conditionally take by reference depending on whether or not the type is
            // copyable.
            function_args.push_str(&format!(
                ", {}: &{}",
                arg.name(),
                arg_ty.get().type_expression()?
            ));

            function_arg_names.push_str(&format!(", {}", arg.name()));

            scope.insert(arg.name(), arg.name().to_string());
        }

        let mut enum_cases = LineBuilder::new();
        let mut parse_cases = LineBuilder::new();
        let mut serialize_cases = LineBuilder::new();

        for case in self.proto.case() {
            let case_type = self.case_types.get(case.name()).unwrap().get();

            let mut arguments = HashMap::new();
            for arg in case.argument() {
                arguments.insert(arg.name(), evaluate_expression(arg.value(), &scope)?);
            }

            let context = TypeParserContext {
                // TODO: REplace this with explcitly refining the 'input' buffer during parsing (so
                // that it is properly inherited for structs inside of strucrs)
                after_bytes: Some("0".to_string()),
                scope: &HashMap::new(),
                arguments: &arguments,
            };

            if !case.comment().is_empty() {
                enum_cases.add(format!("/// {}", case.comment()));
            }

            enum_cases.add(format!(
                "\t{}({}),",
                case.name(),
                case_type.type_expression()?
            ));

            let match_value = {
                if case.is_default() {
                    "_".to_string()
                } else {
                    evaluate_expression(case.case_value(), &scope)?
                }
            };

            parse_cases.add(format!(
                r#"{} => {{
                    let v = {};
                    (Self::{}(v), input)
                }}"#,
                match_value,
                case_type.parse_bytes_expression(&context)?,
                case.name()
            ));

            serialize_cases.add(format!(
                r#"{} => {{
                    let v = match self {{
                        Self::{}(v) => v,
                        _ => return Err(err_msg("Case/switch value mismatch"))
                    }};

                    {}
                }}"#,
                match_value,
                case.name(),
                case_type.serialize_bytes_expression("v", &context)?
            ));
        }

        if !self.has_default {
            parse_cases.add(
                r#"
                _ => {
                    return Err(err_msg("Unable to match any union case")); 
                }
            "#,
            );
        }

        let switch_value = evaluate_expression(self.proto.switch_value(), &scope)?;

        out.add(format!(
            r#"
            #[derive(Debug, Clone)]
            pub enum {name} {{
                {enum_cases}
            }}

            impl {name} {{
                pub fn parse<'a>(mut input: &'a [u8]{function_args}) -> Result<(Self, &'a [u8])> {{
                    let switch_value = {switch_value};

                    Ok(match switch_value.as_ref() {{
                        {parse_cases}
                    }})
                }}

                pub fn serialize(&self, out: &mut Vec<u8>{function_args}) -> Result<()> {{
                    let switch_value = {switch_value};

                    match switch_value.as_ref() {{
                        {serialize_cases}
                    }};

                    Ok(())
                }}
            }}
            "#,
            name = self.proto.name(),
            enum_cases = enum_cases.to_string(),
            parse_cases = parse_cases.to_string(),
            serialize_cases = serialize_cases.to_string(),
            function_args = function_args,
            switch_value = switch_value,
        ));

        Ok(())
    }

    fn type_expression(&self) -> Result<String> {
        Ok(self.proto.name().to_string())
    }

    // TODO: Deduplicate with the struct type
    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String> {
        let mut args = String::new();
        for arg in self.proto.argument() {
            let val = context
                .arguments
                .get(arg.name())
                .ok_or_else(|| format_err!("Argument not provided: {}", arg.name()))?;
            args = format!(", {}", val);
        }

        Ok(format!(
            "parse_next!(input, |i| {}::parse(i{}))",
            self.proto.name(),
            args
        ))
    }

    // TODO: Deduplicate with the struct type
    fn serialize_bytes_expression(
        &self,
        value: &str,
        context: &TypeParserContext,
    ) -> Result<String> {
        let mut args = String::new();
        for arg in self.proto.argument() {
            let val = context
                .arguments
                .get(arg.name())
                .ok_or_else(|| format_err!("Argument not provided: {}", arg.name()))?;
            args = format!(", {}", val);
        }

        Ok(format!("{}.serialize(out{})?;", value, args))
    }

    fn sizeof(&self, field_name: &str) -> Result<Option<SizeExpression>> {
        // TODO: It will only have a well defined size if all the cases (and the default
        // case) have the same size.
        Ok(None)
    }
}
