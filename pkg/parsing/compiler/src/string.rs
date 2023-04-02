use common::errors::*;
use common::line_builder::*;

use crate::buffer::BufferType;
use crate::expression::Expression;
use crate::expression::*;
use crate::proto::*;
use crate::types::*;

pub struct StringType<'a> {
    proto: &'a StringTypeProto,
    buffer_type: BufferType<'a>,
}

impl<'a> StringType<'a> {
    pub fn create(
        proto: &'a StringTypeProto,
        resolver: &mut TypeResolver<'a>,
        context: &TypeResolverContext,
    ) -> Result<Self> {
        if proto.charset() != StringCharset::UTF8 {
            return Err(err_msg("Unsupported charset for string"));
        }

        let buffer_type = BufferType::create(proto.buffer(), resolver, context)?;
        Ok(Self { proto, buffer_type })
    }
}

impl<'a> Type for StringType<'a> {
    fn default_value_expression(&self) -> Result<String> {
        if self.proto.buffer().has_fixed_length() {
            let len = self.proto.buffer().fixed_length();
            Ok("::common::collections::FixedString::default()".to_string())
        } else {
            Ok("String::default()".to_string())
        }
    }

    fn type_expression(&self) -> Result<String> {
        if self.proto.buffer().has_fixed_length() {
            let len = self.proto.buffer().fixed_length();
            Ok(format!("::common::collections::FixedString<[u8; {}]>", len))
        } else {
            Ok("String".to_string())
        }
    }

    fn parse_bytes_expression(&self, context: &TypeParserContext) -> Result<String> {
        let mut lines = LineBuilder::new();
        lines.add("{");

        // First get the buffer.
        lines.add(format!(
            "let data = {};",
            self.buffer_type.parse_bytes_expression(context)?
        ));

        // Wrap in a string.
        if self.proto.buffer().has_fixed_length() {
            lines.add(
                r#"
                let mut out = ::common::collections::FixedString::default();
                out.push_str(::std::str::from_utf8(&data[..])?);
                out
            "#,
            );
        } else {
            lines.add("String::from_utf8(data)?");
        }

        lines.add("}");

        Ok(lines.to_string())
    }

    fn serialize_bytes_expression(
        &self,
        value: &str,
        context: &TypeParserContext,
    ) -> Result<String> {
        self.buffer_type
            .serialize_bytes_expression(&format!("{}.as_ref()", value), context)
    }

    fn size_of(&self, field_name: &str) -> Result<Option<Expression>> {
        self.buffer_type.size_of(field_name)
    }
}
