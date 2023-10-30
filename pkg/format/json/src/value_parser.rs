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

impl<'a> reflection::ValueReader<'a> for ValueParser<'a> {
    fn parse<T: reflection::ParseFromValue<'a>>(self) -> Result<T> {
        match self.value {
            Value::Object(v) => T::parse_from_object(ObjectParser { map: v.iter() }),
            Value::Array(v) => T::parse_from_list(ListParser { values: &v[..] }),
            Value::String(v) => {
                T::parse_from_primitive(reflection::PrimitiveValue::String(v.clone()))
            }
            Value::Number(v) => T::parse_from_primitive(reflection::PrimitiveValue::F64(*v)),
            Value::Bool(v) => T::parse_from_primitive(reflection::PrimitiveValue::Bool(*v)),
            Value::Null => T::parse_from_primitive(reflection::PrimitiveValue::Null),
        }
    }
}

pub struct ObjectParser<'a> {
    map: std::collections::hash_map::Iter<'a, String, Value>,
}

impl<'a> reflection::ObjectIterator<'a> for ObjectParser<'a> {
    type ValueReaderType = ValueParser<'a>;

    fn next_field(&mut self) -> Result<Option<(String, Self::ValueReaderType)>> {
        Ok(self
            .map
            .next()
            .map(|(key, value)| (key.clone(), ValueParser { value })))
    }
}

pub struct ListParser<'a> {
    values: &'a [Value],
}

impl<'a> reflection::ListIterator<'a> for ListParser<'a> {
    type ValueReaderType = ValueParser<'a>;

    fn next(&mut self) -> Result<Option<Self::ValueReaderType>> {
        if self.values.is_empty() {
            return Ok(None);
        }

        let value = &self.values[0];
        self.values = &self.values[1..];
        Ok(Some(ValueParser { value }))
    }
}
