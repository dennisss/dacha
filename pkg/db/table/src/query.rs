use std::collections::HashMap;

use protobuf::reflection::Reflection;
use protobuf::FieldNumber;

#[derive(Default)]
pub struct Query {
    pub any_of: Vec<QueryAllOf>,
}

impl Query {
    pub fn or(&mut self, all_of: QueryAllOf) -> &mut Self {
        self.any_of.push(all_of);
        self
    }
}

#[derive(Default)]
pub struct QueryAllOf {
    // TODO: Make this deterministic.
    pub fields: HashMap<FieldNumber, Vec<QueryOperation>>,
}

impl QueryAllOf {
    pub fn and(&mut self, field: FieldNumber, op: QueryOperation) -> &mut Self {
        self.fields.entry(field).or_default().push(op);
        self
    }
}

pub enum QueryOperation {
    Eq(QueryValue),
    LessThan(QueryValue),
    LessThanOrEqual(QueryValue),
    GreaterThan(QueryValue),
    GreaterThanOrEqual(QueryValue),
}

pub enum QueryValue {
    U32(u32),
    U64(u64),
}

impl QueryValue {
    pub fn reflect<'a>(&'a self) -> Reflection<'a> {
        match self {
            QueryValue::U32(v) => Reflection::U32(v),
            QueryValue::U64(v) => Reflection::U64(v),
        }
    }
}
