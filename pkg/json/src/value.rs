
use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
pub enum Value {
    Object(HashMap<String, Value>),
    Array(Vec<Value>),
    String(String),
    Number(f64),
    Bool(bool),
    Null
}