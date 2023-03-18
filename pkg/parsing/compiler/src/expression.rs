use std::collections::HashMap;

use common::errors::*;
use skylark::syntax::*;

// 'scope' is a map from field/variable names to the Rust expression that
// retrieves its value.
pub fn evaluate_expression(expr: &str, scope: &HashMap<&str, String>) -> Result<String> {
    let (expr, rest) = Expression::parse(expr, &ParsingContext::default())?;
    if !rest.is_empty() {
        return Err(format_err!(
            "Extra unparsed text after expression: {}",
            rest
        ));
    }

    if expr.tests.len() != 1 {
        return Err(err_msg("Expected message to have exactly one test"));
    }

    evaluate_test(&expr.tests[0], scope)
}

fn evaluate_test(test: &Test, scope: &HashMap<&str, String>) -> Result<String> {
    match test {
        Test::If(_) => todo!(),
        Test::Primary(e) => evaluate_primary_expr(&e, scope),
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

            let left = evaluate_test(&e.left, scope)?;
            let right = evaluate_test(&e.right, scope)?;

            Ok(format!("({} {} {})", left, op, right))
        }
    }
}

fn evaluate_primary_expr(e: &PrimaryExpression, scope: &HashMap<&str, String>) -> Result<String> {
    let mut base = match &e.base {
        Operand::Identifier(name) => scope
            .get(name.as_str())
            .ok_or_else(|| format_err!("Unknown field named: {}", name))?
            .clone(),
        Operand::Int(v) => {
            format!("{}", *v)
        }
        op => {
            panic!("Unsupported operant: {:?}", op)
        }
    };

    for s in &e.suffixes {
        match s {
            PrimaryExpressionSuffix::Dot(ident) => {
                // TODO: Validate that these exist in the type.
                base = format!("{}.{}", base, ident);
            }
            _ => todo!(),
        };
    }

    Ok(base)
}
