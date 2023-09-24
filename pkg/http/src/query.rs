// Utilities for dealing with the form parameters encoded in a URL's query
// string or form body also known as 'application/x-www-form-urlencoded'.
//
// The specificication is defined in:
// https://url.spec.whatwg.org/#application/x-www-form-urlencoded

use std::collections::HashMap;
use std::fmt::Write;

use common::errors::*;
use parsing::ascii::AsciiString;
use parsing::opaque::OpaqueString;

// /// Map based storage of query parameters.
// /// This is efficient for lookup but ignores any ordering between values with
// different names. pub struct QueryParams {
//     params: HashMap<OpaqueString, Vec<OpaqueString>>,
// }

pub struct QueryParamsBuilder {
    out: String,
}

impl QueryParamsBuilder {
    pub fn new() -> Self {
        Self { out: String::new() }
    }

    pub fn add(&mut self, key: &[u8], value: &[u8]) -> &mut Self {
        self.out.reserve(key.len() + value.len() + 2);
        if !self.out.is_empty() {
            self.out.push('&');
        }

        self.add_slice(key);
        if !value.is_empty() {
            self.out.push('=');
            self.add_slice(value);
        }

        self
    }

    fn add_slice(&mut self, data: &[u8]) {
        for byte in data.iter().cloned() {
            if byte == b' ' {
                self.out.push('+');
            } else if byte.is_ascii_alphanumeric() {
                // TODO: Also allow some punctionation.
                self.out.push(byte as char);
            } else {
                write!(self.out, "%{:02X}", byte).unwrap();
            }
        }
    }

    pub fn build(self) -> AsciiString {
        AsciiString::from(self.out).unwrap()
    }
}

impl<'a> reflection::ValueSerializer for &'a mut QueryParamsBuilder {
    type ObjectSerializerType = Self;
    type ListSerializerType = Self;

    fn serialize_primitive(self, value: reflection::PrimitiveValue) -> Result<()> {
        Err(err_msg("Must serialize query values in an object"))
    }

    fn serialize_object(self) -> Self::ObjectSerializerType {
        self
    }

    fn serialize_list(self) -> Self::ListSerializerType {
        self
    }
}

impl<'a> reflection::ObjectSerializer for &'a mut QueryParamsBuilder {
    fn serialize_field<Value: reflection::SerializeTo>(
        &mut self,
        name: &str,
        value: &Value,
    ) -> Result<()> {
        if value.serialize_as_empty_value() {
            return Ok(());
        }

        value.serialize_to(QueryFieldSerializer {
            field_name: name,
            builder: self,
        })
    }
}

impl<'a> reflection::ListSerializer for &'a mut QueryParamsBuilder {
    fn serialize_element<Value: reflection::SerializeTo>(&mut self, value: &Value) -> Result<()> {
        Err(err_msg("Can't serialize a list to query params"))
    }
}

struct QueryFieldSerializer<'a> {
    field_name: &'a str,
    builder: &'a mut QueryParamsBuilder,
}

impl<'a> reflection::ValueSerializer for QueryFieldSerializer<'a> {
    type ObjectSerializerType = &'a mut QueryParamsBuilder;
    type ListSerializerType = &'a mut QueryParamsBuilder;

    fn serialize_primitive(self, value: reflection::PrimitiveValue) -> Result<()> {
        use reflection::PrimitiveValue;
        match value {
            PrimitiveValue::Null => todo!(),
            PrimitiveValue::Bool(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::I8(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::U8(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::I16(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::U16(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::I32(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::U32(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::I64(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::U64(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::ISize(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::USize(v) => self
                .builder
                .add(self.field_name.as_bytes(), v.to_string().as_bytes()),
            PrimitiveValue::F32(_) => todo!(),
            PrimitiveValue::F64(_) => todo!(),
            PrimitiveValue::Str(v) => self.builder.add(self.field_name.as_bytes(), v.as_bytes()),
            PrimitiveValue::String(v) => self.builder.add(self.field_name.as_bytes(), v.as_bytes()),
        };

        Ok(())
    }

    fn serialize_object(self) -> Self::ObjectSerializerType {
        // TODO: REturn a null serializer
        todo!()
    }

    fn serialize_list(self) -> Self::ListSerializerType {
        // TODO: Return a null serializer.
        todo!()
    }
}

pub struct QueryParamsParser<'a> {
    input: &'a [u8],
}

impl<'a> QueryParamsParser<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input }
    }

    fn decode_percent_encoded(&mut self) -> Option<u8> {
        if self.input.len() < 2 {
            return None;
        }

        let s = match std::str::from_utf8(&self.input[0..2]) {
            Ok(s) => s,
            Err(_) => {
                return None;
            }
        };

        match u8::from_str_radix(s, 16) {
            Ok(v) => {
                self.input = &self.input[2..];
                Some(v)
            }
            Err(_) => None,
        }
    }
}

impl std::iter::Iterator for QueryParamsParser<'_> {
    type Item = (OpaqueString, OpaqueString);

    fn next(&mut self) -> Option<Self::Item> {
        let mut name = vec![];
        let mut value = vec![];
        let mut parsing_value = false;

        while !self.input.is_empty() && self.input[0] == b'&' {
            self.input = &self.input[1..];
        }

        if self.input.is_empty() {
            return None;
        }

        while !self.input.is_empty() {
            let mut byte = self.input[0];
            self.input = &self.input[1..];

            if byte == b'=' {
                if !parsing_value {
                    parsing_value = true;
                    continue;
                }
            } else if byte == b'&' {
                break;
            }

            if byte == b'+' {
                byte = b' ';
            } else if byte == b'%' {
                if let Some(decoded) = self.decode_percent_encoded() {
                    byte = decoded;
                }
            }

            if parsing_value {
                value.push(byte);
            } else {
                name.push(byte);
            }
        }

        Some((OpaqueString::from(name), OpaqueString::from(value)))
    }
}

/*
impl<'data> reflection::ValueReader<'data> for QueryParamsParser<'_> {
    fn parse<T: reflection::ParseFromValue<'data>>(self) -> Result<T> {
        T::parse_from_object(self)
    }
}

impl<'data> reflection::ObjectIterator<'data> for QueryParamsParser<'_> {
    fn next_field(&mut self) -> Result<Option<(String, Self::ValueReaderType)>> {

    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_test() {
        // TODO: Distinguish between 'name' and 'name='?
        let input = b"&hello=wor=ld&value=123 +go&&=&name&encoded=%333r%ZZ%";
        let raw_expected_outputs: &[(&[u8], &[u8])] = &[
            (b"hello", b"wor=ld"),
            (b"value", b"123  go"),
            (b"", b""),
            (b"name", b""),
            (b"encoded", b"33r%ZZ%"),
        ];

        let expected_outputs = raw_expected_outputs
            .iter()
            .map(|(k, v)| (OpaqueString::from(*k), OpaqueString::from(*v)))
            .collect::<Vec<_>>();

        let outputs = QueryParamsParser::new(input).collect::<Vec<_>>();
        assert_eq!(outputs, expected_outputs);
    }
}
