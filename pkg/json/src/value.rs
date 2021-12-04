use std::collections::HashMap;

use common::errors::*;
use reflection::{ListSerializer, ObjectSerializer};

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    Object(HashMap<String, Value>),
    Array(Vec<Value>),
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

impl Value {
    pub fn get_field(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Object(v) => v.get(name),
            _ => None,
        }
    }

    pub fn get_field_mut(&mut self, name: &str) -> Option<&mut Value> {
        match self {
            Self::Object(v) => v.get_mut(name),
            _ => None,
        }
    }

    pub fn get_element(&self, idx: usize) -> Option<&Value> {
        match self {
            Self::Array(v) => v.get(idx),
            _ => None,
        }
    }

    pub fn get_element_mut(&mut self, idx: usize) -> Option<&mut Value> {
        match self {
            Self::Array(v) => v.get_mut(idx),
            _ => None,
        }
    }

    pub fn get_elements(&self) -> Option<&[Value]> {
        match self {
            Self::Array(v) => Some(&v[..]),
            _ => None,
        }
    }

    pub fn get_string(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s.as_str()),
            _ => None,
        }
    }
}

impl std::ops::Index<usize> for Value {
    type Output = Value;

    fn index(&self, index: usize) -> &Self::Output {
        self.get_element(index).unwrap()
    }
}

impl std::ops::IndexMut<usize> for Value {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        self.get_element_mut(index).unwrap()
    }
}

impl std::ops::Index<&str> for Value {
    type Output = Value;

    fn index(&self, index: &str) -> &Self::Output {
        self.get_field(index).unwrap()
    }
}

impl std::ops::IndexMut<&str> for Value {
    fn index_mut(&mut self, index: &str) -> &mut Self::Output {
        self.get_field_mut(index).unwrap()
    }
}

impl reflection::SerializeTo for Value {
    fn serialize_to<Output: reflection::ValueSerializer>(&self, out: Output) -> Result<()> {
        match self {
            Value::Object(v) => {
                let mut obj = out.serialize_object();
                for (key, value) in v.iter() {
                    obj.serialize_field(key.as_str(), value)?;
                }

                Ok(())
            }
            Value::Array(v) => {
                let mut arr = out.serialize_list();
                for v in v {
                    arr.serialize_element(v)?;
                }

                Ok(())
            }
            Value::String(v) => {
                out.serialize_primitive(reflection::PrimitiveValue::Str(v.as_str()))
            }
            Value::Number(v) => out.serialize_primitive(reflection::PrimitiveValue::F64(*v)),
            Value::Bool(v) => out.serialize_primitive(reflection::PrimitiveValue::Bool(*v)),
            Value::Null => out.serialize_primitive(reflection::PrimitiveValue::Null),
        }
    }
}
