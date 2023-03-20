use common::errors::*;
use common::line_builder::*;

use crate::buffer::BufferType;
use crate::expression::Expression;
use crate::expression::*;
use crate::proto::*;
use crate::types::*;

pub struct LayeredType<'a> {
    proto: &'a LayeredTypeProto,
    inner_type: TypeReference<'a>,
    outer_type: TypeReference<'a>,
}

impl<'a> LayeredType<'a> {
    pub fn create(
        proto: &'a LayeredTypeProto,
        resolver: &mut TypeResolver<'a>,
        context: &TypeResolverContext,
    ) -> Result<Self> {
        let inner_type = resolver.resolve_type(proto.inner(), context)?;
        let outer_type = resolver.resolve_type(proto.outer(), context)?;

        Ok(Self {
            proto,
            inner_type,
            outer_type,
        })
    }
}

impl<'a> Type for LayeredType<'a> {
    fn default_value_expression(&self) -> Result<String> {
        self.outer_type.get().default_value_expression()
    }

    fn type_expression(&self) -> Result<String> {
        self.outer_type.get().type_expression()
    }

    fn value_expression(&self, value: &Value) -> Result<String> {
        self.outer_type.get().value_expression(value)
    }

    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String> {
        Ok(format!(
            r#"
            {{
                let inner = {{ {} }};
                let input = &inner[..];

                {}
            }}
            "#,
            self.inner_type.get().parse_bytes_expression(context)?,
            self.outer_type.get().parse_bytes_expression(context)?
        ))
    }

    fn serialize_bytes_expression(
        &self,
        value: &str,
        output_buffer: &str,
        context: &TypeParserContext,
    ) -> Result<String> {
        // TODO: Should serialize without temporary buffers (or just serialize to cords)
        Ok(format!(
            r#"
            {{
                let v = {{
                    let mut temp = vec![];
                    {}
                    temp
                }};

                {}
            }}
            "#,
            self.outer_type
                .get()
                .serialize_bytes_expression(value, "(&mut temp)", context)?,
            self.inner_type
                .get()
                .serialize_bytes_expression("v", output_buffer, context)?
        ))
    }

    fn size_of(&self, field_name: &str) -> Result<Option<Expression>> {
        // TODO
        Ok(None)
    }
}
