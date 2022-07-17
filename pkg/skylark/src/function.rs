use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use common::errors::*;

use crate::object::*;
use crate::scope::Scope;
use crate::value::Value;

pub struct FunctionValue {
    /// Value bound to the first argument ('self') of this function.
    pub instance: Option<ObjectWeak<dyn Value>>,

    ///
    pub def: FunctionDef,
}

impl FunctionValue {
    pub fn from_builtin<F: BuiltinFunction>(f: F) -> Self {
        FunctionValue {
            instance: None,
            def: FunctionDef::Builtin(Box::new(f)),
        }
    }
}

impl Value for FunctionValue {
    fn test_value(&self) -> bool {
        true
    }

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        if let Some(inst) = &self.instance {
            out.push(inst.clone());
        }
    }

    /// Immutable
    fn freeze_value(&self) {}

    fn python_str(&self) -> String {
        match &self.def {
            FunctionDef::Builtin(f) => format!("<built-in function {}>", f.name()),
        }
    }
}

pub enum FunctionDef {
    Builtin(Box<dyn BuiltinFunction>),
}

pub trait BuiltinFunction: 'static + Send + Sync {
    fn name(&self) -> String;

    fn call(&self, context: FunctionCallContext) -> Result<ObjectStrong<dyn Value>>;
}

pub struct FunctionCallContext {
    pub scope: Arc<Scope>,

    /// Pool which should be used for creating objects returned by the function.
    pub pool: ObjectPool<dyn Value>,

    pub args: Vec<FunctionArgument>,
}

pub struct FunctionArgument {
    pub name: Option<String>,
    pub value: ObjectStrong<dyn Value>,
}

pub struct FunctionArgumentIterator {
    state: FunctionArgumentIteratorState,
    positional_args: VecDeque<ObjectStrong<dyn Value>>,

    // TODO: Expose as an ordered dict if passing back to python.
    keyword_args: HashMap<String, ObjectStrong<dyn Value>>,
}

// NOTE: This changes purely based on what the caller of the
// FunctionArgumentIterator methods does.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FunctionArgumentIteratorState {
    SinglePositionalArgs = 1,
    DonePositionalArgs = 2,
    SingleKeywordArgs = 3,
    DoneKeywordArgs = 4,
}

impl<'a> FunctionArgumentIterator {
    pub fn create(args: &[FunctionArgument]) -> Result<Self> {
        let mut last_positional = None;
        let mut positional_args = VecDeque::new();
        let mut keyword_args = HashMap::new();

        for (i, arg) in args.iter().enumerate() {
            if let Some(name) = &arg.name {
                if keyword_args
                    .insert(name.clone(), arg.value.clone())
                    .is_some()
                {
                    return Err(err_msg("Duplicate keyword argument passed to function"));
                }
            } else {
                if let Some(last_i) = last_positional {
                    if last_i + 1 != i {
                        return Err(err_msg(
                            "Not allowed to specify positional arguments after keyword arguments",
                        ));
                    }
                }

                positional_args.push_back(arg.value.clone());
                last_positional = Some(i);
            }
        }

        Ok(Self {
            state: FunctionArgumentIteratorState::SinglePositionalArgs,
            positional_args,
            keyword_args,
        })
    }

    pub fn next_positional_arg(&mut self, name: &str) -> Result<Option<ObjectStrong<dyn Value>>> {
        if self.state != FunctionArgumentIteratorState::SinglePositionalArgs {
            return Err(err_msg("Argument iterator called in bad order"));
        }

        if let Some(value) = self.keyword_args.remove(name) {
            if !self.positional_args.is_empty() {
                return Err(err_msg("If an argument is passed with a keyword, then all arguments after it must also be keyword args"));
            }

            return Ok(Some(value));
        }

        if let Some(value) = self.positional_args.pop_front() {
            return Ok(Some(value));
        }

        Ok(None)
    }

    pub fn required_positional_arg(&mut self, name: &str) -> Result<ObjectStrong<dyn Value>> {
        self.next_positional_arg(name)?
            .ok_or_else(|| format_err!("No value specified for argument: {}", name))
    }

    pub fn remaining_positional_args(&mut self) -> Result<Vec<ObjectStrong<dyn Value>>> {
        // This function can only be called once and only before we start asking for
        // keyword arguments.
        if self.state as usize >= FunctionArgumentIteratorState::DonePositionalArgs as usize {
            return Err(err_msg("Argument iterator called in bad order"));
        }
        self.state = FunctionArgumentIteratorState::DonePositionalArgs;

        Ok(self.positional_args.drain(0..).collect())
    }

    /// Get the next keyword argument with the given name and a unknown
    /// position.
    ///
    /// This will never be used by functions Python defined
    /// functions as they always fully define the order of arguments, but with
    /// builtin functions, it is possible to define functions with no ordering
    /// of arguments.
    pub fn next_keyword_arg(&mut self, name: &str) -> Result<Option<ObjectStrong<dyn Value>>> {
        // This function can be called after any positonal argument function.
        if (self.state as usize) < (FunctionArgumentIteratorState::SingleKeywordArgs as usize) {
            self.state = FunctionArgumentIteratorState::SingleKeywordArgs;
        }

        // This function can't be called after remaining_keyword_args().
        if self.state != FunctionArgumentIteratorState::SingleKeywordArgs {
            return Err(err_msg("Argument iterator called in bad order"));
        }

        if !self.positional_args.is_empty() {
            return Err(err_msg("Extra unparsed positional arguments"));
        }

        if let Some(value) = self.keyword_args.remove(name) {
            return Ok(Some(value));
        }

        Ok(None)
    }

    pub fn required_keyword_arg(&mut self, name: &str) -> Result<ObjectStrong<dyn Value>> {
        self.next_keyword_arg(name)?
            .ok_or_else(|| format_err!("No value specified for argument: {}", name))
    }

    // TODO: Implement this and return a DictValue wrapped in a ObjectStrong<dyn
    // Value> pub fn remaining_keyword_args(&mut self, )

    pub fn finish(self) -> Result<()> {
        if !self.positional_args.is_empty() {
            return Err(err_msg("Extra unparsed positional arguments"));
        }

        if !self.keyword_args.is_empty() {
            return Err(err_msg("Extra unparsed positional arguments"));
        }

        Ok(())
    }
}
