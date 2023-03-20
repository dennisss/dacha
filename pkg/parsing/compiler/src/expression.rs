use std::collections::HashMap;

use common::errors::*;
use skylark::syntax::*;

use crate::types::TypeReference;

/*
Some things are well defined right now:
- Some things aren't

- During serialization, have all values, but may not know everything like sizeof(field_name)


For every field, we have some things we can do:
-


I'd like to try and evaluate an expression



*/

/*
Easiest to

*/

pub struct Symbol<'a> {
    pub typ: TypeReference<'a>,

    /// Rust expression that evaluates to the value of this symbol.
    pub value: Option<String>,

    /// Rust expression that
    pub size_of: Option<String>,
}

#[derive(Clone, Debug)]
pub enum Expression {
    Integer(i64),
    String(String),
    Field(FieldExpression),
    BinaryOp(BinaryOpExpression),
    List(Vec<Expression>),
}

#[derive(Clone, Debug)]
pub struct FieldExpression {
    pub field_path: Vec<String>,
    pub attribute: Attribute,
}

#[derive(Clone, Debug)]
pub enum Attribute {
    ValueOf,
    SizeOf,
    Length,
}

#[derive(Clone, Debug)]
pub struct BinaryOpExpression {
    pub op: &'static str,
    pub left: Box<Expression>,
    pub right: Box<Expression>,
}

impl Expression {
    pub fn parse(expr: &str) -> Result<Self> {
        let (expr, rest) = skylark::syntax::Expression::parse(expr, &ParsingContext::default())?;
        if !rest.is_empty() {
            return Err(format_err!(
                "Extra unparsed text after expression: {}",
                rest
            ));
        }

        if expr.tests.len() != 1 {
            return Err(err_msg("Expected message to have exactly one test"));
        }

        Self::parse_test(&expr.tests[0])
    }

    fn parse_test(test: &Test) -> Result<Self> {
        match test {
            Test::If(_) => todo!(),
            Test::Primary(e) => Self::parse_primary_expr(e),
            Test::Unary(_) => todo!(),
            Test::Binary(e) => {
                let op = match e.op {
                    BinaryOp::Or => "||",
                    BinaryOp::And => "&&",
                    BinaryOp::IsEqual => "==",
                    BinaryOp::NotEqual => "!=",
                    BinaryOp::LessThan => "<",
                    BinaryOp::GreaterThan => ">",
                    BinaryOp::LessEqual => "<=",
                    BinaryOp::GreaterEqual => ">=",
                    BinaryOp::BitOr => "|",
                    BinaryOp::Xor => "^",
                    BinaryOp::BitAnd => "&",
                    BinaryOp::Subtract => "-",
                    BinaryOp::Add => "+",
                    BinaryOp::Multiply => "*",
                    _ => todo!(),
                };

                let left = Self::parse_test(&e.left)?;
                let right = Self::parse_test(&e.right)?;

                Ok(Self::BinaryOp(BinaryOpExpression {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                }))
            }
        }
    }

    fn parse_primary_expr(e: &PrimaryExpression) -> Result<Self> {
        let mut idents = vec![];

        match &e.base {
            Operand::Identifier(name) => idents.push(name.clone()),
            Operand::Int(v) => {
                return Ok(Self::Integer(*v));
            }
            Operand::String(v) => {
                return Ok(Self::String(v.clone()));
            }
            Operand::List(expr) => {
                let mut items = vec![];
                for test in &expr.tests {
                    items.push(Self::parse_test(test)?);
                }

                return Ok(Self::List(items));
            }
            op => {
                panic!("Unsupported operand: {:?}", op)
            }
        };

        for (i, s) in e.suffixes.iter().enumerate() {
            match s {
                PrimaryExpressionSuffix::Dot(ident) => {
                    // TODO: Validate that these exist in the type.
                    // base = format!("{}.{}", base, ident);

                    idents.push(ident.clone());
                }
                PrimaryExpressionSuffix::Call(args) => {
                    if !args.is_empty() {
                        return Err(err_msg("Only zero argument functions supported"));
                    }

                    let fname = idents.pop().unwrap();
                    if idents.is_empty() {
                        return Err(err_msg("Global functions not supported"));
                    }

                    // TODO: Validate that these don't colide with any field names.
                    match fname.as_str() {
                        "size_of" => {
                            return Ok(Expression::Field(FieldExpression {
                                field_path: idents,
                                attribute: Attribute::SizeOf,
                            }));
                        }
                        "len" => {
                            return Ok(Expression::Field(FieldExpression {
                                field_path: idents,
                                attribute: Attribute::Length,
                            }));
                        }
                        _ => return Err(format_err!("Unsupported function: {}", fname)),
                    }

                    if i != e.suffixes.len() {
                        // We don't support things like 'x.size_of() sizeof(x)(y).z'
                        return Err(err_msg("Expected call to be the last in the epxression"));
                    }
                }
                _ => todo!(),
            };
        }

        Ok(Expression::Field(FieldExpression {
            field_path: idents,
            attribute: Attribute::ValueOf,
        }))
    }

    pub fn add(self, other: Self) -> Self {
        Self::BinaryOp(BinaryOpExpression {
            op: "+",
            left: Box::new(self),
            right: Box::new(other),
        })
    }

    pub fn mul(self, other: Self) -> Self {
        Self::BinaryOp(BinaryOpExpression {
            op: "*",
            left: Box::new(self),
            right: Box::new(other),
        })
    }

    /// Wraps all field references inside of the given field.
    pub fn scoped(mut self, field_name: &str) -> Self {
        match &mut self {
            Self::Field(ref mut e) => e.field_path.insert(0, field_name.to_string()),
            Self::BinaryOp(ref mut op) => {
                op.left = Box::new(op.left.clone().scoped(field_name));
                op.right = Box::new(op.right.clone().scoped(field_name));
            }
            Self::List(ref mut exprs) => {
                for e in exprs {
                    *e = e.clone().scoped(field_name);
                }
            }
            Self::Integer(_) | Self::String(_) => {}
        }

        self
    }

    pub fn evaluate(&self, scope: &HashMap<&str, Symbol>) -> Result<Option<String>> {
        Ok(Some(match self {
            Expression::Integer(v) => format!("{:?}", *v),
            Expression::String(v) => format!("{:?}", *v),
            Expression::BinaryOp(e) => {
                let left = match e.left.evaluate(scope) {
                    Ok(Some(v)) => v,
                    e => return e,
                };

                let right = match e.right.evaluate(scope) {
                    Ok(Some(v)) => v,
                    e => return e,
                };

                format!("({} {} {})", left, e.op, right)
            }
            Expression::List(items) => {
                let mut out = "[".to_string();
                for item in items {
                    let s = match item.evaluate(scope) {
                        Ok(Some(v)) => v,
                        v => return v,
                    };

                    out.push_str(s.as_str());
                    out.push_str(", ");
                }
                out.push_str("]");
                out
            }
            Expression::Field(v) => {
                let symbol_name = v.field_path[0].as_str();
                let symbol = scope
                    .get(symbol_name)
                    .ok_or_else(|| format_err!("Unknown field named: {}", symbol_name))?;
                // TODO: Also validate that all inner identifiers are also valid fields.

                match v.attribute {
                    Attribute::ValueOf | Attribute::Length => {
                        let mut expr = match &symbol.value {
                            Some(v) => v.clone(),
                            None => return Ok(None),
                        };

                        for field in &v.field_path[1..] {
                            expr.push_str(&format!(".{}", field));
                        }

                        // TODO: Must validate that the type is a buffer or string.
                        if let Attribute::Length = v.attribute {
                            expr = format!("{}.len()", expr);
                        }

                        expr
                    }
                    Attribute::SizeOf => {
                        let expr = match &symbol.size_of {
                            Some(v) => v.clone(),
                            None => return Ok(None),
                        };

                        if v.field_path.len() != 1 {
                            return Err(err_msg(
                                "size_of currently onlt supported for top level fields",
                            ));
                        }

                        expr
                    }
                }
            }
        }))
    }

    pub fn to_constant(&self) -> Option<i64> {
        match self {
            Expression::Integer(v) => Some(*v),
            Expression::String(_) => None,

            // TODO: If the field has a constant value or size, we may be able to evaluate it here.
            Expression::Field(_) => None,

            Expression::List(_) => None,

            Expression::BinaryOp(op) => {
                let left = match op.left.to_constant() {
                    Some(v) => v,
                    None => return None,
                };
                let right = match op.right.to_constant() {
                    Some(v) => v,
                    None => return None,
                };

                Some(match op.op {
                    "+" => left + right,
                    "-" => left - right,
                    "*" => left * right,
                    "/" => left / right,
                    _ => return None,
                })
            }
        }
    }
}

// Size expressions are similarly very tricky.
// I'd like 'length' to also be an expression
// - Once a field is serialized or de-serilized,

// 'scope' is a map from field/variable names to the Rust expression that
// retrieves its value.
