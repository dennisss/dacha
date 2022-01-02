use alloc::string::ToString;
use alloc::vec::Vec;

use crate::args::{ArgType, RawArgValue};
use crate::errors::*;

pub struct CommaSeparated<T> {
    pub values: Vec<T>,
    hidden: (),
}

impl<T: ArgType + Sized> ArgType for CommaSeparated<T> {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self> {
        let mut values = vec![];

        match raw_arg {
            RawArgValue::Bool(v) => {
                values.push(T::parse_raw_arg(RawArgValue::Bool(v))?);
            }
            RawArgValue::String(v) => {
                for s in v.split(",") {
                    values.push(T::parse_raw_arg(RawArgValue::String(s.to_string()))?);
                }
            }
        }

        Ok(Self { values, hidden: () })
    }

    fn parse_optional_raw_arg(raw_arg: Option<RawArgValue>) -> Result<Self> {
        let arg = match raw_arg {
            Some(v) => v,
            None => {
                return Ok(Self {
                    values: vec![],
                    hidden: (),
                });
            }
        };

        Self::parse_raw_arg(arg)
    }
}
