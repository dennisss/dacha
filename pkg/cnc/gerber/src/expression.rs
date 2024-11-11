use std::collections::HashMap;

use base_error::*;

use crate::syntax::*;

#[derive(Default)]
pub struct ExpressionEvaluator {
    vars: HashMap<String, f64>,
}

impl ExpressionEvaluator {
    pub fn add_call_params(&mut self, params: &[f64]) {
        for (i, v) in params.iter().cloned().enumerate() {
            self.vars.insert(format!("${}", i + 1), v);
        }
    }

    pub fn define_variable(&mut self, name: &str, value: &Expression) -> Result<()> {
        let v = self.evaluate(value)?;
        self.vars.insert(name.to_string(), v);
        Ok(())
    }

    pub fn evaluate(&self, expr: &Expression) -> Result<f64> {
        Ok(match expr {
            Expression::Number(v) => *v,
            Expression::Variable(v) => *self
                .vars
                .get(v)
                .ok_or_else(|| format_err!("No variable defiend with name: {}", v))?,
            Expression::BinaryOp(op, left_expr, right_expr) => {
                let left = self.evaluate(left_expr.as_ref())?;
                let right = self.evaluate(right_expr.as_ref())?;

                match op {
                    BinaryOp::Add => left + right,
                    BinaryOp::Subtract => left - right,
                    BinaryOp::Multiply => left * right,
                    BinaryOp::Divide => left / right,
                }
            }
        })
    }
}
