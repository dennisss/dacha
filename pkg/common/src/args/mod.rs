/// An implementation of CLI flags that is mostly compatible with Abseil.
///
/// Usage:
/// let my_bool = Arg::<bool>::required("enabled");
/// let my_string = Arg::<string>::optional("path", "/dev/null");
/// common::args::init(&[&my_bool, &my_string])?;
///
/// my_bool.value()

use std::any::Any;
use std::cell::Ref;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::Deref;
use std::rc::Rc;
use std::string::ToString;

use crate::errors::*;


pub struct RawArgs {
    positional: Vec<String>,

    // If a None is stored, then this argument was already taken by a previous call.
    named_args: HashMap<String, RawArgValue>,

    requested_args: HashSet<String>
}

pub enum RawArgValue {
    String(String),
    Bool(bool),
}

impl RawArgs {

    fn create_from_env() -> Result<Self> {
        let mut escaped_mode = false;

        let mut named_args = HashMap::new();
        let mut positional_args = vec![];
        for arg_str in std::env::args().skip(1) {
            if escaped_mode {
                positional_args.push(arg_str);
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
    
                    if named_args.insert(key.to_string(), RawArgValue::String(value.to_string())).is_some() {
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
    
                    if named_args.insert(key.to_string(), RawArgValue::Bool(value)).is_some() {
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
            requested_args: HashSet::new()
        })
    }

    fn is_empty(&self) -> bool {
        self.positional.is_empty() && self.named_args.is_empty()
    }

    pub fn next_positional_arg(&mut self) -> Result<String> {
        if self.positional.is_empty() {
            Err(err_msg("Expected additional positional argument"))
        } else {
            Ok(self.positional.remove(0))
        }
    }

    pub fn take_remaining_positional_args(&mut self) -> Vec<String> {
        self.positional.split_off(0)
    }

    pub fn take_named_arg(&mut self, name: &str) -> Result<Option<RawArgValue>> {
        if !self.requested_args.insert(name.to_string()) {
            return Err(err_msg("Duplicate definition of argument"))
        }

        Ok(self.named_args.remove(name))
    }
}



pub trait ArgsType {
    fn parse_raw_args(raw_args: &mut RawArgs) -> Result<Self> where Self: Sized;
}

pub trait ArgType {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self> where Self: Sized;

    fn parse_optional_raw_arg(raw_arg: Option<RawArgValue>) -> Result<Self> where Self: Sized {
        let value = raw_arg.ok_or_else(|| err_msg("Missing value for argument"))?;
        Self::parse_raw_arg(value)
    }
}

impl ArgType for String {
    fn parse_raw_arg(raw_arg: RawArgValue) -> Result<Self> {
        match raw_arg {
            RawArgValue::Bool(_) => { Err(err_msg("Expected string, got bool")) }
            RawArgValue::String(s) => { Ok(s) }
        }
    } 
}





pub fn parse_args<Args: ArgsType + Sized>() -> Result<Args> {
    let mut raw_args = RawArgs::create_from_env()?;
    let args = Args::parse_raw_args(&mut raw_args)?;

    if !raw_args.is_empty() {
        return Err(err_msg("Unused arguments present"));
    }

    Ok(args)
}
