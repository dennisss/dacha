use alloc::string::{String, ToString};
use alloc::vec::Vec;

use base_error::*;

use crate::{ArgFieldType, ArgType, ArgsType, RawArgValue, RawArgs};
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

pub struct EscapedArgs {
    pub args: Vec<String>,
}

impl ArgsType for EscapedArgs {
    fn parse_raw_args(raw_args: &mut super::RawArgs) -> Result<Self> {
        let args = raw_args.take_escaped_args();
        Ok(Self { args })
    }
}

impl ArgFieldType for EscapedArgs {
    fn parse_raw_arg_field(field_name: &str, raw_args: &mut RawArgs) -> Result<Self> {
        Self::parse_raw_args(raw_args)
    }
}
