use core::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use common::errors::*;

use crate::function::*;
use crate::object::*;
use crate::scope::*;
use crate::syntax::*;
use crate::value::*;

/// Shared builtins/values available to any file.
pub struct Universe {
    pool: ObjectPool<dyn Value>,
    scope: Arc<Scope>,
}

impl Universe {
    pub fn new() -> Result<Self> {
        let pool = ObjectPool::new();
        let scope = Scope::new("", &pool, None)?;

        Ok(Self { pool, scope })
    }

    /// Allocate a new value in the universe's object pool.
    pub fn insert<V: Value>(&self, value: V) -> Result<ObjectStrong<dyn Value>> {
        self.pool.insert(value)
    }

    /// Binds a named variable value in the universe scope. Unless shadowed,
    /// this value will be visible to all files being evaluated.
    pub fn bind(&self, name: &str, value: ObjectWeak<dyn Value>) -> Result<()> {
        self.scope.bindings().insert(name, value)
    }
}

/// Evaluates
pub struct Environment {
    universe: Universe,
}

#[derive(Clone, Copy)]
struct EvaluationContext<'a> {
    scope: &'a Arc<Scope>,
    pool: &'a ObjectPool<dyn Value>,
}

impl Environment {
    pub fn new(universe: Universe) -> Result<Self> {
        universe.pool.freeze()?;

        Ok(Self { universe })
    }

    /// Evaluates a single skylark file.
    ///
    /// Arguments:
    /// - source_path: File path to the file being evaluated.
    /// - source: Contents of the file being evaluated.
    pub fn evaluate_file(&self, source_path: &str, source: &str) -> Result<()> {
        let mut file_pool = ObjectPool::new();
        let mut file_scope =
            Scope::new(source_path, &file_pool, Some(self.universe.scope.clone()))?;

        let context = EvaluationContext {
            scope: &file_scope,
            pool: &file_pool,
        };

        let file = File::parse(source)?;
        for statement in file.statements {
            match statement {
                Statement::Def(_) => todo!(),

                Statement::Expression(e) => {
                    let _ = self.evaluate_expression(&e, false, context)?;
                }
                Statement::Continue | Statement::Break | Statement::Pass | Statement::Return(_) => {
                    return Err(err_msg("Unexpected statement"));
                }
            }
        }

        Ok(())
    }

    /// NOTE: An expression can be parsed either as a
    fn evaluate_expression(
        &self,
        expr: &Expression,
        in_list: bool,
        context: EvaluationContext,
    ) -> Result<ObjectStrong<dyn Value>> {
        let mut tests = vec![];

        for test in &expr.tests {
            tests.push(self.evaluate_test(test, context)?);
        }

        let value = if in_list {
            context.pool.insert(ListValue::new(
                tests.iter().map(|o| o.downgrade()).collect(),
            ))?
        } else {
            if tests.len() == 1 && !expr.has_trailing_comma {
                tests.pop().unwrap()
            } else {
                // TODO: Cache the empty tuple value.

                context.pool.insert(TupleValue::new(
                    tests.iter().map(|o| o.downgrade()).collect(),
                ))?
            }
        };

        Ok(value)
    }

    fn evaluate_test(
        &self,
        test: &Test,
        context: EvaluationContext,
    ) -> Result<ObjectStrong<dyn Value>> {
        match test {
            Test::If(e) => {
                // TODO: We may need to garbage collect this.
                let cond = self.evaluate_test(&e.condition, context)?;

                if cond.test_value() {
                    self.evaluate_test(&e.true_value, context)
                } else {
                    self.evaluate_test(&e.false_value, context)
                }
            }
            Test::Primary(e) => {
                let mut base = match &e.base {
                    Operand::Identifier(name) => context
                        .scope
                        .resolve(name)?
                        .ok_or_else(|| format_err!("No variable named: {}", name))?,
                    Operand::Int(v) => context.pool.insert(IntValue::new(*v))?,
                    Operand::Float(v) => context.pool.insert(FloatValue::new(*v))?,
                    Operand::String(v) => context.pool.insert(StringValue::new(v.clone()))?,
                    Operand::List(v) => todo!(),
                    Operand::Dict(_) => todo!(),
                    Operand::Tuple(_) => todo!(),
                };

                // TODO: Mutate 'base' here
                for suffix in &e.suffixes {
                    match suffix {
                        PrimaryExpressionSuffix::Dot(_) => todo!(),
                        PrimaryExpressionSuffix::Call(raw_args) => {
                            base = self.evaluate_function_call(&*base, &raw_args[..], context)?;
                        }
                    }
                }

                Ok(base)
            }
            Test::Unary(_) => todo!(),
            Test::Binary(_) => todo!(),
        }
    }

    fn evaluate_function_call(
        &self,
        func_ptr: &dyn Value,
        raw_args: &[Argument],
        context: EvaluationContext,
    ) -> Result<ObjectStrong<dyn Value>> {
        let func_value = match func_ptr.as_any().downcast_ref::<FunctionValue>() {
            Some(v) => v,
            None => {
                return Err(err_msg("Only allowed to call a function"));
            }
        };

        let mut func_args = vec![];
        func_args.reserve_exact(raw_args.len() + if func_value.instance.is_some() { 1 } else { 0 });

        // TODO: Disallow normal arguments after keyword args.

        if let Some(val) = &func_value.instance {
            func_args.push(FunctionArgument {
                name: None,
                value: val
                    .upgrade()
                    .ok_or_else(|| err_msg("Dangling pointer in function call"))?,
            });
        }

        for arg in raw_args {
            match arg {
                Argument::Value(v) => {
                    func_args.push(FunctionArgument {
                        name: None,
                        value: self.evaluate_test(v, context)?,
                    });
                }
                Argument::KeyValue(key, value) => func_args.push(FunctionArgument {
                    name: Some(key.clone()),
                    value: self.evaluate_test(value, context)?,
                }),
                Argument::Variadic(_) => todo!(),
                Argument::KeywordArgs(_) => todo!(),
            }
        }

        // TODO: Make a child scope just for this file
        let func_context = FunctionCallContext {
            scope: context.scope.clone(),
            pool: context.pool.clone(),
            args: func_args,
        };

        match &func_value.def {
            FunctionDef::Builtin(f) => {
                return f.call(func_context);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// In python3 this has the signature:
    ///   print(*objects, sep=' ', end='\n', file=sys.stdout, flush=False)
    /// See https://docs.python.org/3/library/functions.html#print
    struct InMemoryPrintFunc {
        buffer: Arc<Mutex<String>>,
    }

    impl BuiltinFunction for InMemoryPrintFunc {
        fn name(&self) -> String {
            "print".to_string()
        }

        fn call(&self, context: FunctionCallContext) -> Result<ObjectStrong<dyn Value>> {
            let mut args = FunctionArgumentIterator::create(&context.args)?;

            let objects = args.remaining_positional_args()?;

            let sep_object = args.next_keyword_arg("sep")?;
            let sep = match &sep_object {
                Some(v) => v
                    .downcast_string()
                    .ok_or_else(|| err_msg("Expected 'sep' to be a string"))?,
                None => " ",
            };

            let end_object = args.next_keyword_arg("end")?;
            let end = match &end_object {
                Some(v) => v
                    .downcast_string()
                    .ok_or_else(|| err_msg("Expected 'end' to be a string"))?,
                None => "\n",
            };

            // TODO: Ensure this is always called via a Drop check.
            args.finish()?;

            let mut buffer = self.buffer.lock().unwrap();

            for (i, obj) in objects.iter().enumerate() {
                if i > 0 {
                    buffer.push_str(sep);
                }

                buffer.push_str(obj.python_str().as_str());
            }

            buffer.push_str(end);

            Ok(context.pool.insert(NoneValue::new())?)
        }
    }

    struct AdderFunc {}

    impl BuiltinFunction for AdderFunc {
        fn name(&self) -> String {
            "adder".to_string()
        }

        fn call(&self, context: FunctionCallContext) -> Result<ObjectStrong<dyn Value>> {
            let mut args = FunctionArgumentIterator::create(&context.args)?;
            let a = args
                .required_positional_arg("a")?
                .downcast_int()
                .ok_or_else(|| err_msg("Expected int for argument 'a'"))?;
            let b = args
                .required_positional_arg("b")?
                .downcast_int()
                .ok_or_else(|| err_msg("Expected int for argument 'b'"))?;
            args.finish()?;

            println!("Called: {} + {}", a, b);

            Ok(context.pool.insert(IntValue::new(a + b))?)
        }
    }

    #[test]
    fn works() -> Result<()> {
        let mut universe = Universe::new()?;
        {
            let func_value = universe.insert(FunctionValue::from_builtin(AdderFunc {}))?;
            universe.bind("adder", func_value.downgrade())?;
        }

        let mut stdout = Arc::new(Mutex::new(String::new()));
        {
            let func_value = universe.insert(FunctionValue::from_builtin(InMemoryPrintFunc {
                buffer: stdout.clone(),
            }))?;
            universe.bind("print", func_value.downgrade())?;
        }

        let mut env = Environment::new(universe)?;

        env.evaluate_file("my_file", "print(\'I got:\', adder(a=1, b=2))")?;

        let stdout = stdout.lock().unwrap().clone();

        assert_eq!(stdout, "I got: 3\n");

        Ok(())
    }
}
