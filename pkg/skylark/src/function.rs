use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use common::errors::*;

use crate::dict::*;
use crate::object::*;
use crate::primitives::*;
use crate::scope::Scope;
use crate::value::{Value, ValueCallFrame};
use crate::value_attributes;

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

    pub fn wrap<
        F: 'static + Send + Sync + Fn(FunctionCallContext) -> Result<ObjectStrong<dyn Value>>,
    >(
        name: &str,
        f: F,
    ) -> Self {
        Self::from_builtin(BuiltinLambdaFunction::wrap(name, f))
    }
}

impl Value for FunctionValue {
    value_attributes!(Immutable | ReprAsStr);

    fn referenced_value_objects(&self, out: &mut Vec<ObjectWeak<dyn Value>>) {
        if let Some(inst) = &self.instance {
            out.push(inst.clone());
        }
    }

    fn call_bool(&self) -> bool {
        true
    }

    fn call_repr(&self, context: &mut ValueCallFrame) -> Result<String> {
        Ok(match &self.def {
            FunctionDef::Builtin(f) => format!("<built-in function {}>", f.name()),
        })
    }

    fn call_hash(
        &self,
        hasher: &mut dyn crypto::hasher::Hasher,
        context: &mut ValueCallFrame,
    ) -> Result<()> {
        Err(err_msg("Please don't hash functions"))
    }

    fn call_eq(&self, other: &dyn Value, context: &mut ValueCallFrame) -> Result<bool> {
        Ok(core::ptr::eq::<dyn Value>(self, other))
    }
}

pub enum FunctionDef {
    Builtin(Box<dyn BuiltinFunction>),
}

pub trait BuiltinFunction: 'static + Send + Sync {
    fn name(&self) -> String;

    fn call(&self, context: FunctionCallContext) -> Result<ObjectStrong<dyn Value>>;
}

/// TODO: Pass all of these as &'a pointers.
pub struct FunctionCallContext<'a, 'b> {
    pub frame: &'a mut ValueCallFrame<'b>,

    pub scope: Arc<Scope>,

    pub args: Vec<FunctionArgument>,
}

impl<'a, 'b> FunctionCallContext<'a, 'b> {
    pub fn pool(&self) -> &ObjectPool<dyn Value> {
        self.frame.pool()
    }
}

pub struct FunctionArgument {
    pub name: Option<String>,
    pub value: ObjectStrong<dyn Value>,
}

pub struct FunctionArgumentIterator<'a, 'b> {
    frame: &'a mut ValueCallFrame<'b>,
    state: FunctionArgumentIteratorState,
    positional_args: VecDeque<ObjectStrong<dyn Value>>,
    keyword_args: ObjectStrong<dyn Value>,
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

impl<'a, 'b> FunctionArgumentIterator<'a, 'b> {
    pub fn create(args: &[FunctionArgument], frame: &'a mut ValueCallFrame<'b>) -> Result<Self> {
        let mut last_positional = None;
        let mut positional_args = VecDeque::new();

        let keyword_args = frame.pool().insert(DictValue::default())?;
        let keyword_args_dict = keyword_args.as_any().downcast_ref::<DictValue>().unwrap();

        for (i, arg) in args.iter().enumerate() {
            if let Some(name) = &arg.name {
                let key = frame.pool().insert(StringValue::new(name.clone()))?;

                let mut inner_frame = frame.child(keyword_args_dict)?;

                if keyword_args_dict
                    .insert(&key, arg.value.downgrade(), &mut inner_frame)?
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
            frame,
            state: FunctionArgumentIteratorState::SinglePositionalArgs,
            positional_args,
            keyword_args,
        })
    }

    fn keyword_args(&self) -> &DictValue {
        self.keyword_args
            .as_any()
            .downcast_ref::<DictValue>()
            .unwrap()
    }

    pub fn next_positional_arg(&mut self, name: &str) -> Result<Option<ObjectStrong<dyn Value>>> {
        if self.state != FunctionArgumentIteratorState::SinglePositionalArgs {
            return Err(err_msg("Argument iterator called in bad order"));
        }

        let mut inner_frame = self.frame.child(&*self.keyword_args)?;
        let keyword_args = self
            .keyword_args
            .as_any()
            .downcast_ref::<DictValue>()
            .unwrap();

        if let Some(value) =
            keyword_args.remove(&StringValue::new(name.to_string()), &mut inner_frame)?
        {
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
    /// Python arguments may have an unknown position if they follow argument.
    /// e.g. 'def func(*args, a=None, b=None)'
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

        let key = StringValue::new(name.to_string());
        let mut inner_frame = self.frame.child(&*self.keyword_args)?;
        let keyword_args = self
            .keyword_args
            .as_any()
            .downcast_ref::<DictValue>()
            .unwrap();

        if let Some(value) = keyword_args.remove(&key, &mut inner_frame)? {
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

    /// Captures all remaining keyword args and returns them as a DictValue
    /// object.
    pub fn remaining_keyword_args(&mut self) -> Result<ObjectStrong<dyn Value>> {
        if self.state as usize >= FunctionArgumentIteratorState::DoneKeywordArgs as usize {
            return Err(err_msg("Argument iterator called in bad order"));
        }
        self.state = FunctionArgumentIteratorState::DoneKeywordArgs;

        Ok(self.keyword_args.clone())
    }

    pub fn finish(self) -> Result<()> {
        if !self.positional_args.is_empty() {
            return Err(err_msg("Extra unparsed positional arguments"));
        }

        if !self.keyword_args().is_empty()
            && self.state != FunctionArgumentIteratorState::DoneKeywordArgs
        {
            return Err(err_msg("Extra unparsed keyword arguments"));
        }

        Ok(())
    }

    /// Interprate all arguments as fields of a protobuf message.
    ///
    /// - We expect all arguments to be provided with a keyword corresponding to
    ///   a protobuf field name,
    /// - The fields are proto merged with any existing data in 'message'.
    pub fn to_proto(mut self, output_proto: &mut dyn protobuf::reflection::Reflect) -> Result<()> {
        // TODO: Instead just read all the fields as a kwargs DictValue and pass that
        // completely to value_to_proto.

        let kwargs = self.remaining_keyword_args()?;

        crate::proto::value_to_proto(&*kwargs, output_proto.reflect_mut(), self.frame)?;

        self.finish()
    }
}

pub struct BuiltinLambdaFunction<F> {
    name: String,
    f: F,
}

impl<F> BuiltinLambdaFunction<F> {
    pub fn wrap(name: &str, f: F) -> Self {
        Self {
            name: name.to_string(),
            f,
        }
    }
}

impl<F: 'static + Send + Sync + Fn(FunctionCallContext) -> Result<ObjectStrong<dyn Value>>>
    BuiltinFunction for BuiltinLambdaFunction<F>
{
    fn name(&self) -> String {
        self.name.clone()
    }

    fn call(&self, context: FunctionCallContext) -> Result<ObjectStrong<dyn Value>> {
        (self.f)(context)
    }
}
