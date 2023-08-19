use core::any::Any;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use common::errors::*;

use crate::dict::*;
use crate::function::*;
use crate::list::*;
use crate::object::*;
use crate::primitives::*;
use crate::scope::*;
use crate::syntax::*;
use crate::tuple::*;
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

        let mut inst = Self { pool, scope };

        inst.bind("NotImplemented", NotImplementedValue::new())?;

        inst.bind_function("len", |ctx| {
            let mut args = FunctionArgumentIterator::create(&ctx.args, ctx.frame)?;
            let obj = args.required_positional_arg("s")?;
            args.finish()?;

            let mut inner_frame = ctx.frame.child(&*obj)?;
            let len = obj.call_len(&mut inner_frame)?;
            drop(inner_frame);

            ctx.pool().insert(IntValue::new(len as i64))
        })?;

        Ok(inst)
    }

    /// Allocate a new value in the universe's object pool.
    pub fn insert<V: Value>(&self, value: V) -> Result<ObjectStrong<dyn Value>> {
        self.pool.insert(value)
    }

    /// Binds a named variable value in the universe scope. Unless shadowed,
    /// this value will be visible to all files being evaluated.
    pub fn bind<V: Value>(&self, name: &str, value: V) -> Result<()> {
        let key = self.insert(StringValue::new(name.to_string()))?;
        let value = self.insert(value)?;

        // NOTE: None of these universe methods should ever be called during evaluation
        // so it is ok to start a new call stack.
        let mut parent_pointers = ValuePointers::default();
        let mut root_call_context = ValueCallFrame::root(&self.pool, &mut parent_pointers);

        let mut ctx = root_call_context.child(self.scope.bindings())?;

        self.scope
            .bindings()
            .insert(&key, value.downgrade(), &mut ctx)?;
        Ok(())
    }

    pub fn bind_function<
        F: 'static + Send + Sync + Fn(FunctionCallContext) -> Result<ObjectStrong<dyn Value>>,
    >(
        &self,
        name: &str,
        f: F,
    ) -> Result<()> {
        self.bind(name, FunctionValue::wrap(name, f))?;
        Ok(())
    }
}

/// Evaluates
pub struct Environment {
    universe: Universe,
}

// #[derive(Clone, Copy)]
struct EvaluationContext<'a> {
    scope: Arc<Scope>,
    frame: ValueCallFrame<'a>,
}

impl EvaluationContext<'_> {
    fn pool(&self) -> &ObjectPool<dyn Value> {
        self.frame.pool()
    }
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

        // Used to track recursion.
        let mut parent_pointers = ValuePointers::default();

        let root_call_context = ValueCallFrame::root(&file_pool, &mut parent_pointers);

        let mut context = EvaluationContext {
            scope: file_scope,
            frame: root_call_context,
        };

        let file = File::parse(source)?;
        for statement in file.statements {
            match statement {
                Statement::Def(_) => todo!(),

                Statement::Expression(e) => {
                    let _ = self.evaluate_expression(&e, false, &mut context)?;
                }
                Statement::Continue | Statement::Break | Statement::Pass | Statement::Return(_) => {
                    return Err(err_msg("Unexpected statement"));
                }
                Statement::Assign { target, op, value } => {
                    // TODO: Basically implement the target evaluation by constructing special
                    // reference values.

                    todo!()
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
        context: &mut EvaluationContext,
    ) -> Result<ObjectStrong<dyn Value>> {
        let mut tests = vec![];

        for test in &expr.tests {
            tests.push(self.evaluate_test(test, context)?);
        }

        let value = if in_list {
            context.pool().insert(ListValue::new(
                tests.iter().map(|o| o.downgrade()).collect(),
            ))?
        } else {
            if tests.len() == 1 && !expr.has_trailing_comma {
                tests.pop().unwrap()
            } else {
                // TODO: Cache the empty tuple value.

                context.pool().insert(TupleValue::new(
                    tests.iter().map(|o| o.downgrade()).collect(),
                ))?
            }
        };

        Ok(value)
    }

    fn evaluate_test(
        &self,
        test: &Test,
        context: &mut EvaluationContext,
    ) -> Result<ObjectStrong<dyn Value>> {
        match test {
            Test::If(e) => {
                // TODO: We may need to garbage collect this.
                let cond = self.evaluate_test(&e.condition, context)?;

                if cond.call_bool() {
                    self.evaluate_test(&e.true_value, context)
                } else {
                    self.evaluate_test(&e.false_value, context)
                }
            }
            Test::Primary(e) => {
                let mut base =
                    match &e.base {
                        Operand::Identifier(name) => context
                            .scope
                            .resolve(name, &mut context.frame)?
                            .ok_or_else(|| format_err!("No variable named: {}", name))?,
                        Operand::Int(v) => context.pool().insert(IntValue::new(*v))?,
                        Operand::Float(v) => context.pool().insert(FloatValue::new(*v))?,
                        Operand::String(v) => context.pool().insert(StringValue::new(v.clone()))?,
                        Operand::Bool(v) => context.pool().insert(BoolValue::new(*v))?,
                        Operand::None => context.pool().insert(NoneValue::new())?,
                        Operand::List(v) => self.evaluate_expression(v, true, context)?,
                        Operand::Dict(entries) => {
                            // NOTE: We create the object first to ensure that the inner values
                            // aren't GC'ed
                            let dict_object = context.pool().insert(DictValue::default())?;
                            let dict = dict_object.as_any().downcast_ref::<DictValue>().unwrap();

                            for (key_test, value_test) in entries {
                                let key = self.evaluate_test(key_test, context)?;
                                let value = self.evaluate_test(value_test, context)?;

                                let mut dict_context = context.frame.child(dict)?;

                                // Per the https://bazel.build/rules/language#differences_with_python page, literal dicts can't have
                                if dict
                                    .insert(&key, value.downgrade(), &mut dict_context)?
                                    .is_some()
                                {
                                    return Err(err_msg("Duplicate key present in dict literal"));
                                }
                            }

                            dict_object
                        }
                        Operand::Tuple(_) => todo!(),
                    };

                // TODO: Mutate 'base' here
                for suffix in &e.suffixes {
                    match suffix {
                        PrimaryExpressionSuffix::Dot(_) => todo!(),
                        PrimaryExpressionSuffix::Call(raw_args) => {
                            base = self.evaluate_function_call(&*base, &raw_args[..], context)?;
                        }
                        PrimaryExpressionSuffix::Slice(_) => todo!(),
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
        context: &mut EvaluationContext,
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

        let mut inner_call_context = context.frame.child(func_ptr)?;

        // TODO: Make a child scope just for this file
        let func_context = FunctionCallContext {
            frame: &mut inner_call_context,
            scope: context.scope.clone(),
            args: func_args,
        };

        match &func_value.def {
            FunctionDef::Builtin(f) => {
                return f.call(func_context);
            }
        };

        drop(inner_call_context);
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
            let mut args = FunctionArgumentIterator::create(&context.args, context.frame)?;

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

                let mut inner_context = context.frame.child(&**obj)?;
                buffer.push_str(obj.call_str(&mut inner_context)?.as_str());
            }

            buffer.push_str(end);

            Ok(context.pool().insert(NoneValue::new())?)
        }
    }

    struct AdderFunc {}

    impl BuiltinFunction for AdderFunc {
        fn name(&self) -> String {
            "adder".to_string()
        }

        fn call(&self, context: FunctionCallContext) -> Result<ObjectStrong<dyn Value>> {
            let mut args = FunctionArgumentIterator::create(&context.args, context.frame)?;
            let a = args
                .required_positional_arg("a")?
                .downcast_int()
                .ok_or_else(|| err_msg("Expected int for argument 'a'"))?;
            let b = args
                .required_positional_arg("b")?
                .downcast_int()
                .ok_or_else(|| err_msg("Expected int for argument 'b'"))?;
            args.finish()?;

            Ok(context.pool().insert(IntValue::new(a + b))?)
        }
    }

    #[test]
    fn works() -> Result<()> {
        let mut universe = Universe::new()?;
        universe.bind("adder", FunctionValue::from_builtin(AdderFunc {}))?;

        let mut stdout = Arc::new(Mutex::new(String::new()));
        universe.bind(
            "print",
            FunctionValue::from_builtin(InMemoryPrintFunc {
                buffer: stdout.clone(),
            }),
        )?;

        let mut env = Environment::new(universe)?;

        env.evaluate_file("my_file", "print(\'I got:\', adder(a=len([3,4,5]), b=2))")?;

        let stdout = stdout.lock().unwrap().clone();

        assert_eq!(stdout, "I got: 5\n");

        Ok(())
    }

    #[test]
    fn proto_conversion() -> Result<()> {
        use protobuf::proto::test::*;

        let output = Arc::new(Mutex::new(Vec::<ShoppingList>::new()));

        let mut universe = Universe::new()?;
        let mut output_copy = output.clone();
        universe.bind_function("shopping_list", move |ctx| {
            let mut args = FunctionArgumentIterator::create(&ctx.args, ctx.frame)?;

            let mut proto = ShoppingList::default();
            args.to_proto(&mut proto)?;

            output_copy.lock().unwrap().push(proto);

            ctx.pool().insert(NoneValue::new())
        });

        let mut env = Environment::new(universe)?;

        env.evaluate_file(
            "my_file",
            r#"shopping_list(
    # Testing comments!
    name = "groceries",
    # Here too!
    id = 12,
    cost = 15.99,
    items = [
        { "name": "granny smith", "fruit_type": "APPLES" },
        {},
        { "name": "cherry", "fruit_type": "BERRIES" }
    ]
)"#,
        )?;

        let mut outputs = output.lock().unwrap();

        assert_eq!(outputs.len(), 1);

        assert_eq!(
            protobuf::text::serialize_text_proto(&outputs[0]),
            r#"name: "groceries"
id: 12
cost: 15.99
items: [
    {
        name: "granny smith"
        fruit_type: APPLES
    },
    {},
    {
        name: "cherry"
        fruit_type: BERRIES
    }
]
"#
        );

        println!("============================");
        println!("{}", protobuf::text::serialize_text_proto(&outputs[0]));
        println!("============================");

        Ok(())
    }

    // TODO: '[a, b] = (1,2)' is ok.
    // TODO: '[a], [b] = ((3,), (4,))' is ok.
    // TODO: '{ 'a': 2, 'a': 3 }' is ok
    // TODO: a.hello = 'world'
}
