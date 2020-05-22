/// An implementation of CLI flags that is mostly compatible with Abseil.
///
/// Usage:
/// let my_bool = Arg::<bool>::required("enabled");
/// let my_string = Arg::<string>::optional("path", "/dev/null");
/// common::args::init(&[&my_bool, &my_string])?;
///
/// my_bool.value()
use crate::errors::*;
use std::any::Any;
use std::cell::Ref;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::string::ToString;

#[derive(Clone)]
pub struct Arg<T> {
    name: String,
    desc: String,
    value: RefCell<Option<T>>,
}

// TODO: Disallow starting a name with 'no'

impl<T> Arg<T> {
    pub fn required<S: ToString>(name: S) -> Self {
        Self::new(name.to_string(), None)
    }

    pub fn optional<S: ToString, V: Into<T>>(name: S, default_value: V) -> Self {
        Self::new(name.to_string(), Some(default_value.into()))
    }

    pub fn desc<S: ToString>(mut self, desc: S) -> Self {
        self.desc = desc.to_string();
        self
    }

    fn new(name: String, state: Option<T>) -> Self {
        assert!(!name.starts_with("no"));
        Self {
            name,
            desc: String::new(),
            value: RefCell::new(state),
        }
    }

    pub fn borrow(&self) -> Ref<T> {
        Ref::map(self.value.borrow(), |opt| opt.as_ref().unwrap())
    }
}

pub trait ArgHandler {
    /// Returns the name of the argument handled by this handler
    fn name(&self) -> &str;
    fn parse_str(&self, value: &str) -> Result<()>;
    fn parse_bool(&self, value: bool) -> Result<()>;
    fn has_value(&self) -> bool;
}

impl<T: std::str::FromStr> ArgHandler for Arg<T>
where
    <T as std::str::FromStr>::Err: 'static + Send + Sync + failure::Fail,
{
    default fn name(&self) -> &str {
        &self.name
    }

    default fn parse_str(&self, value: &str) -> Result<()> {
        *(self.value.try_borrow_mut()?) = Some(value.parse()?);
        Ok(())
    }

    default fn parse_bool(&self, value: bool) -> Result<()> {
        Err(format_err!(
            "Argument '{}' can not be parsed as bool",
            self.name()
        ))
    }

    default fn has_value(&self) -> bool {
        self.value.borrow().is_some()
    }
}

impl ArgHandler for Arg<bool> {
    fn parse_str(&self, value: &str) -> Result<()> {
        if value.eq_ignore_ascii_case("true") {
            self.parse_bool(true)
        } else if value.eq_ignore_ascii_case("false") {
            self.parse_bool(false)
        } else {
            Err(format_err!("Could not parse '{}' as bool", value))
        }
    }

    fn parse_bool(&self, value: bool) -> Result<()> {
        *(self.value.try_borrow_mut()?) = Some(value);
        Ok(())
    }
}

impl ArgHandler for &dyn ArgHandler {
    fn name(&self) -> &str {
        (*self).name()
    }

    fn parse_str(&self, value: &str) -> Result<()> {
        (*self).parse_str(value)
    }

    fn parse_bool(&self, value: bool) -> Result<()> {
        (*self).parse_bool(value)
    }

    fn has_value(&self) -> bool {
        (*self).has_value()
    }
}

pub fn init(args: &[&dyn ArgHandler]) -> Result<()> {
    let mut arg_map = HashMap::<&str, &dyn ArgHandler>::new();
    for arg in args.iter() {
        arg_map.insert(arg.name(), arg);
    }

    let mut escaped_mode = false;
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

                let arg = arg_map
                    .get(key)
                    .ok_or(format_err!("Unknown argument named '{}'", key))?;
                arg.parse_str(value)?;
            } else {
                let mut key = arg_tuple;
                let value = if let Some(k) = key.strip_prefix("no") {
                    key = k;
                    false
                } else {
                    true
                };

                let arg = arg_map
                    .get(key)
                    .ok_or(format_err!("Unknown argument named '{}'", key))?;
                arg.parse_bool(value)?;
            }
        } else {
            positional_args.push(arg_str);
        }
    }

    for arg in args.iter() {
        if !arg.has_value() {
            return Err(format_err!("Argument '{}' is missing a value.", arg.name()));
        }
    }

    Ok(())
}
