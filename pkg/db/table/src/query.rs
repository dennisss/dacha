use std::cmp::Ordering;
use std::collections::HashMap;

use base_error::*;
use protobuf::reflection::Reflection;
use protobuf::{FieldNumber, MessageReflection};

use crate::reflection::field_by_path;

/// Query criteria used to match rows in a table (equivalent to an SQL 'WHERE'
/// clause).
///
/// Internally this is stored in an OR of AND'ed conditions.
///
/// Some important edge cases:
/// - { any_of: [] } means that nothing will be matched
/// - { any_of: [ { fields: {} } ] } means that everything will be matched.
#[derive(Default)]
pub struct Query {
    pub(crate) any_of: Vec<QueryAllOf>,
}

impl Query {
    pub fn or(&mut self, all_of: QueryAllOf) -> &mut Self {
        self.any_of.push(all_of);
        self
    }

    pub fn matches(&self, message: &dyn MessageReflection) -> Result<bool> {
        for all_of in &self.any_of {
            let mut all = true;

            for (field_path, comps) in &all_of.fields {
                let value = field_by_path(message, &field_path[..])?;

                for comp in comps {
                    let rhs = comp.rhs.reflect();
                    let order = compare_reflection(value, rhs)
                        .ok_or_else(|| err_msg("Unable to compare with value"))?;

                    let res = match comp.op {
                        QueryOp::Eq => order.is_eq(),
                        QueryOp::LessThan => order.is_lt(),
                        QueryOp::LessThanOrEqual => order.is_le(),
                        QueryOp::GreaterThan => order.is_gt(),
                        QueryOp::GreaterThanOrEqual => order.is_ge(),
                    };

                    if !res {
                        all = false;
                        break;
                    }
                }

                if !all {
                    break;
                }
            }

            if all {
                return Ok(true);
            }
        }

        Ok(false)
    }
}

#[derive(Default)]
pub(crate) struct QueryAllOf {
    // TODO: Make this deterministic.
    pub(crate) fields: HashMap<Vec<FieldNumber>, Vec<QueryComparison>>,
}

impl QueryAllOf {
    pub fn and(&mut self, field: &[FieldNumber], comp: QueryComparison) -> &mut Self {
        self.fields.entry(field.to_vec()).or_default().push(comp);
        self
    }
}

pub struct QueryComparison {
    pub op: QueryOp,
    pub rhs: QueryValue,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QueryOp {
    Eq,
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Clone)]
pub enum QueryValue {
    I32(i32),
    I64(i64),
    U32(u32),
    U64(u64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
}

impl QueryValue {
    pub fn reflect<'a>(&'a self) -> Reflection<'a> {
        match self {
            QueryValue::I32(v) => Reflection::I32(v),
            QueryValue::I64(v) => Reflection::I64(v),
            QueryValue::U32(v) => Reflection::U32(v),
            QueryValue::U64(v) => Reflection::U64(v),
            QueryValue::Bool(v) => Reflection::Bool(v),
            QueryValue::String(v) => Reflection::String(v.as_str()),
            QueryValue::Bytes(v) => Reflection::Bytes(&v[..]),
        }
    }
}

impl From<u32> for QueryValue {
    fn from(value: u32) -> Self {
        Self::U32(value)
    }
}

impl From<u64> for QueryValue {
    fn from(value: u64) -> Self {
        Self::U64(value)
    }
}

impl From<String> for QueryValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl<'a> From<&'a str> for QueryValue {
    fn from(value: &'a str) -> Self {
        Self::String(value.to_string())
    }
}

impl<'a> From<&'a String> for QueryValue {
    fn from(value: &'a String) -> Self {
        Self::String(value.to_string())
    }
}

/// 65-bit integer used as a normalized representation to allow comparing i64 to
/// u64 types.
#[derive(PartialEq, PartialOrd)]
struct I65(bool, u64);

impl I65 {
    fn from_reflection(value: Reflection) -> Option<Self> {
        Some(match value {
            Reflection::I32(v) => Self(*v >= 0, *v as i64 as u64),
            Reflection::I64(v) => Self(*v >= 0, *v as u64),
            Reflection::U32(v) => Self(true, *v as u64),
            Reflection::U64(v) => Self(true, *v),
            _ => return None,
        })
    }
}

fn normalize_bytes_like<'a>(value: Reflection<'a>) -> Option<&'a [u8]> {
    Some(match value {
        Reflection::String(v) => v.as_bytes(),
        Reflection::Bytes(v) => v,
        _ => return None,
    })
}

fn compare_reflection(a: Reflection, b: Reflection) -> Option<Ordering> {
    if let Some(a) = I65::from_reflection(a) {
        if let Some(b) = I65::from_reflection(b) {
            return a.partial_cmp(&b);
        }
    }

    if let Some(a) = normalize_bytes_like(a) {
        if let Some(b) = normalize_bytes_like(b) {
            return a.partial_cmp(b);
        }
    }

    if let Reflection::Bool(a) = a {
        if let Reflection::Bool(b) = b {
            return a.partial_cmp(b);
        }
    }

    if let Reflection::Enum(a) = a {
        if let Reflection::Enum(b) = b {
            // TODO: Must ensure they are the same type?
            return a.value().partial_cmp(&b.value());
        }
    }

    None
}
