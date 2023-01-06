//! An implementation of CLI flags that is mostly compatible with Abseil.
//!
//! Usage:
//! let my_bool = Arg::<bool>::required("enabled");
//! let my_string = Arg::<string>::optional("path", "/dev/null");
//! common::args::init(&[&my_bool, &my_string])?;
//!
//! my_bool.value()

pub mod list;

#[cfg(feature = "alloc")]
use alloc::string::String;
#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use std::any::Any;
use std::cell::Ref;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Deref;
use std::rc::Rc;
use std::string::ToString;

use failure::ResultExt;

use crate::errors::*;

/// Collection of uninterprated flags from the command line.
pub struct RawArgs {
    positional: Vec<String>,

    // If a None is stored, then this argument was already taken by a previous call.
    named_args: HashMap<String, RawArgValue>,

    /// List of all arguments passed after receiving a '--' argument.
    escaped_args: Vec<String>,

    /// Names of all named arguments which we have already tried to take from
    /// this object.
    requested_args: HashSet<String>,
}

/// Value of a positional argument in RawArgs.
pub enum RawArgValue {
    String(String),
    Bool(bool),
}

impl RawArgs {
    fn create_from_env() -> Result<Self> {
        let mut escaped_mode = false;

        let mut named_args = HashMap::new();
        let mut positional_args = vec![];
        let mut escaped_args = vec![];
        for arg_str in std::env::args().skip(1) {
            if escaped_mode {
                escaped_args.push(arg_str);
                continue;
            }

            if let Some(arg_tuple) = arg_str.strip_prefix("--") {
                if arg_tuple.is_empty() {
                    escaped_mode = true;
                    continue;
                }

                if let Some(pos) = arg_tuple.find('=') {
                    let key = &arg_tuple[0..pos];
                    let value = &arg_tuple[(pos + 1)..];

                    if named_args
                        .insert(key.to_string(), RawArgValue::String(value.to_string()))
                        .is_some()
                    {
                        return Err(format_err!("Duplicate argument named: {}", key));
                    }
                } else {
                    let mut key = arg_tuple;
                    let value = if let Some(k) = key.strip_prefix("no") {
                        key = k;
                        false
                    } else {
                        true
                    };

                    if named_args
                        .insert(key.to_string(), RawArgValue::Bool(value))
                        .is_some()
                    {
                        return Err(format_err!("Duplicate argument named: {}", key));
                    }
                }
            } else {
                positional_args.push(arg_str);
            }
        }

        Ok(Self {
            named_args,
            positional: positional_args,
            escaped_args,
            requested_args: HashSet::new(),
        })
    }

    pub fn is_empty(&self) -> bool {
        self.positional.is_empty() && self.named_args.is_empty() && self.escaped_args.is_empty()
    }

    pub fn next_positional_arg(&mut self) -> Result<String> {
        if self.positional.is_empty() {
            Err(err_msg("Expected additional positional argument"))
        } else {
            Ok(self.positional.remove(0))
        }
    }

    /// TODO: Prevent calling this twice.
    pub fn take_escaped_args(&mut self) -> Vec<String> {
        self.escaped_args.split_off(0)
    }

    pub fn take_named_arg(&mut self, name: &str) -> Result<Option<RawArgValue>> {
        if !self.requested_args.insert(name.to_string()) {
            return Err(err_msg("Duplicate definition of argument"));
        }

        Ok(self.named_args.remove(name))
    }
}

/// Trait implemented by a collection of multiple arguments.
pub trait ArgsType {
    fn parse_raw_args(raw_args: &mut RawArgs) -> Result<Self>
    where
        Self: Sized;
}

/// Trait implemented by a type which stores the value of a single argument.
pub trait ArgType {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self>
    where
        Self: Sized;

    fn parse_optional_raw_arg(raw_arg: Option<RawArgValue>) -> Result<Self>
    where
        Self: Sized,
    {
        let value = raw_arg.ok_or_else(|| err_msg("Missing value for argument"))?;
        Self::parse_raw_arg(value)
    }
}

/// Trait implemented by a type which can be a named field in a struct
/// containing arguments.
pub trait ArgFieldType {
    fn parse_raw_arg_field(field_name: &str, raw_args: &mut RawArgs) -> Result<Self>
    where
        Self: Sized;
}

impl<T: ArgType + Sized> ArgFieldType for T {
    fn parse_raw_arg_field(field_name: &str, raw_args: &mut RawArgs) -> Result<Self> {
        let value = raw_args.take_named_arg(field_name)?;
        Ok(Self::parse_optional_raw_arg(value)
            .with_context(|e| format_err!("For field: {}: {}", field_name, e))?)
    }
}

impl ArgType for String {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self> {
        match raw_arg {
            RawArgValue::Bool(_) => Err(err_msg("Expected string, got bool")),
            RawArgValue::String(s) => Ok(s),
        }
    }
}

impl ArgType for bool {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self> {
        match raw_arg {
            RawArgValue::Bool(v) => Ok(v),
            RawArgValue::String(_) => Err(err_msg("Expected bool, got string")),
        }
    }
}

impl<T: ArgType> ArgType for Option<T> {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self> {
        Ok(Some(T::parse_raw_arg(raw_arg)?))
    }

    fn parse_optional_raw_arg(raw_arg: Option<RawArgValue>) -> Result<Self>
    where
        Self: Sized,
    {
        if let Some(value) = raw_arg {
            Ok(Self::parse_raw_arg(value)?)
        } else {
            Ok(None)
        }
    }
}

impl ArgType for std::path::PathBuf {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self> {
        match raw_arg {
            RawArgValue::Bool(_) => Err(err_msg("Expected string, got bool")),
            RawArgValue::String(s) => Ok(std::path::PathBuf::from(s)),
        }
    }
}

macro_rules! impl_arg_type_from_str {
    ($name:ty) => {
        impl ArgType for $name {
            fn parse_raw_arg(raw_arg: RawArgValue) -> Result<$name> {
                let s = match raw_arg {
                    RawArgValue::Bool(_) => {
                        return Err(err_msg("Expected string, got bool"));
                    }
                    RawArgValue::String(s) => s,
                };

                Ok(s.parse::<$name>()?)
            }
        }
    };
}

impl_arg_type_from_str!(u8);
impl_arg_type_from_str!(i8);
impl_arg_type_from_str!(u16);
impl_arg_type_from_str!(i16);
impl_arg_type_from_str!(u32);
impl_arg_type_from_str!(i32);
impl_arg_type_from_str!(u64);
impl_arg_type_from_str!(i64);
impl_arg_type_from_str!(usize);
impl_arg_type_from_str!(isize);

pub fn parse_args<Args: ArgsType + Sized>() -> Result<Args> {
    let mut raw_args = RawArgs::create_from_env()?;
    let args = Args::parse_raw_args(&mut raw_args)?;

    if !raw_args.is_empty() {
        return Err(err_msg("Unused arguments present"));
    }

    Ok(args)
}
