/*
    Let's make a basic plotting engine.

    - Everything drawable should ideally output a BBOX
    - But for this, we will need to

*/

use common::errors::{err_msg, Result};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::rc::Rc;

pub const ZERO: Expr = Expr::Number(0.0);
pub const ONE: Expr = Expr::Number(1.0);

// Collecting like terms
// Multiplying numbers
// Reducing rationals
// X*0 = 0
// X + 0 = X
// X^0 = 1
// X^1 = X
// (X^n)^m = X^(n*m)
// X*1 = X
// X^a * X^b = X^(a + b)

// Need replacement rules:
// -

/*
Solving equations:
- Multiple/Divide both sides by a common term
- Add/Subtract both sides by a common term.
- Factoring to get roots?
-

- 2*x = 4



*/

pub enum Expr {
    /// Special symbol only to be used in replacement rules for any
    Any,
    Number(f64),
    Special(SpecialExpr),
    Symbol(String),
    Add(Vec<Rc<Self>>),
    Mul(Vec<Rc<Self>>),
    Pow {
        base: Rc<Self>,
        exponent: Rc<Self>,
    },
    Func(FunctionExpr),
}

// (X + Y)&

impl Expr {
    pub fn diff(&self, var: &str) -> Result<Rc<Self>> {
        Ok(Rc::new(match self {
            Expr::Number(_) | Expr::Special(_) => ZERO,
            Expr::Symbol(name) => {
                if &name == var {
                    ONE
                } else {
                    ZERO
                }
            }
            Expr::Add(args) => {
                let mut out = vec![];
                for arg in args {
                    out.push(arg.diff(var)?);
                }

                Expr::Add(out)
            }
            Expr::Mul(args) => {
                let mut sum = vec![];
                for i in 0..args.len() {
                    let mut prod = vec![];
                    for j in 0..args.len() {
                        prod.push(if i == j {
                            args[i].diff(var)?
                        } else {
                            args[i].clone()
                        });
                    }

                    sum.push(Rc::new(Expr::Mul(prod)));
                }

                Expr::Add(sum)
            }
            Expr::Pow { base, exponent } => {
                {
                    let base_is_var = if let Expr::Symbol(name) = base.as_ref() {
                        name == var
                    } else {
                        false
                    };

                    let exp_is_num = if let Expr::Number(_) = exponent.as_ref() {
                        true
                    } else {
                        false
                    };
                }
                // where exponent != -1

                if base_is_var && exp_is_num {}

                Err(err_msg("Unknown derivative"))
                // (exponent - 1) * Derivative(Base) *
            }
        }))
    }

    pub fn simplify(&self) -> Rc<Self> {
        Rc::new(match self {
            Expr::Mul(args) => {
                let mut out = vec![];
                out.reserve(args.len());

                let mut coeff = 1.0;
                for arg in args {
                    let arg = arg.simplify();
                    // X*0 => 0
                    if let Expr::Number(0.0) = arg.as_ref() {
                        return arg;
                    }
                    // X*1 => 1
                    else if let Expr::Number(1.0) = arg.as_ref() {
                        continue;
                    } else if let Expr::Number(n) = arg.as_ref() {
                        coeff *= *n;
                    }
                    // X*(Y*Z) => X*Y*Z
                    else if let Expr::Mul(inner_args) = arg.as_ref() {
                        out.extend_from_slice(inner_args);
                    } else {
                        out.push(arg);
                    }
                    // TODO: X*(A + B) => A*X + B*X ?
                }

                if coeff != 1.0 {
                    out.push(Rc::new(Expr::Number(coeff)));
                }

                if out.len() == 0 {
                    return Rc::new(ONE.clone());
                }

                Expr::Mul(out)
            }
            _ => self.clone(),
        })
    }

    pub fn evalf(&self, vars: &Variables) -> Result<f64> {
        Ok(match self {
            Expr::Number(v) => v,
            Expr::Special(special) => match special {
                SpecialExpr::Pi => std::f64::consts::PI,
                SpecialExpr::I => return Err(err_msg("Result is un-real")),
                SpecialExpr::E => std::f64::consts::E,
            },
            Expr::Symbol(name) => {
                let v = vars.get(name).ok_or(err_msg("Undefined variable"));
                *v
            }
            Expr::Add(args) => {
                let mut sum = 0.0;
                for arg in args {
                    sum += arg.evalf(vars)?;
                }
                sum
            }
            Expr::Mul(args) => {
                let mut prod = 1.0;
                for arg in args {
                    prod *= arg.evalf(vars)?;
                }
                prod
            }
            Expr::Pow { base, exponent } => {
                let base = base.evalf(vars)?;
                let exp = exponent.evalf(vars)?;
                base.powf(exp)
            }
            Expr::Func(f) => match f {
                FunctionExpr::Sin(arg) => arg.evalf(vars)?.sin(),
                FunctionExpr::Cos(arg) => arg.evalf(vars)?.cos(),
                FunctionExpr::Tan(arg) => arg.evalf(vars)?.tan(),
                FunctionExpr::Abs(arg) => arg.evalf(vars)?.abs(),
            },
            _ => Err(err_msg("Unknown expression")),
        })
    }
}

pub enum SpecialExpr {
    Pi,
    E,
    I,
}

pub enum FunctionExpr {
    Sin(Rc<Expr>),
    Cos(Rc<Expr>),
    Tan(Rc<Expr>),
    Abs(Rc<Expr>),
    //    Factorial(Rc<Expr>),
}

pub type Variables = std::collections::HashMap<String, f64>;

//pub trait Expr: Debug + Clone {
//    fn evaluate(&self) -> Rc<dyn Expr>;
//}
//
//pub struct Add {
//    nodes: Vec<Rc<dyn Expr>>,
//}
