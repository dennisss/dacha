use std::collections::HashMap;

use common::errors::*;

use crate::value::Value;

pub struct ValueParser<'a> {
    value: &'a Value,
}

impl<'a> ValueParser<'a> {
    pub fn new(value: &'a Value) -> Self {
        Self { value }
    }
}

impl<'a> reflection::ValueParser<'a> for ValueParser<'a> {
    type ListParserType = ListParser<'a>;
    type ObjectParserType = ObjectParser<'a>;

    fn parse(self) -> Result<reflection::Value<'a, Self::ObjectParserType, Self::ListParserType>> {
        Ok(match self.value {
            Value::Object(v) => reflection::Value::Object(ObjectParser { map: v.iter() }),
            Value::Array(v) => reflection::Value::List(ListParser { values: &v[..] }),
            Value::String(v) => {
                reflection::Value::Primitive(reflection::PrimitiveValue::String(v.clone()))
            }
            Value::Number(v) => reflection::Value::Primitive(reflection::PrimitiveValue::F64(*v)),
            Value::Bool(v) => reflection::Value::Primitive(reflection::PrimitiveValue::Bool(*v)),
            Value::Null => reflection::Value::Primitive(reflection::PrimitiveValue::Null),
        })
    }
}

pub struct ObjectParser<'a> {
    map: std::collections::hash_map::Iter<'a, String, Value>,
}

impl<'a> reflection::ObjectParser<'a> for ObjectParser<'a> {
    type Key = &'a str;
    type ValueParserType = ValueParser<'a>;

    fn next_field(&mut self) -> Result<Option<(Self::Key, Self::ValueParserType)>> {
        Ok(self
            .map
            .next()
            .map(|(key, value)| (key.as_str(), ValueParser { value })))
    }
}

pub struct ListParser<'a> {
    values: &'a [Value],
}

impl<'a> reflection::ListParser<'a> for ListParser<'a> {
    type ValueParserType = ValueParser<'a>;

    fn next(&mut self) -> Option<Self::ValueParserType> {
        if self.values.is_empty() {
            return None;
        }

        let value = &self.values[0];
        self.values = &self.values[1..];
        Some(ValueParser { value })
    }
}