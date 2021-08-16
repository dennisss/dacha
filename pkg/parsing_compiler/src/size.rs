use std::collections::{HashMap, HashSet};

use crate::proto::dsl::Field;

/// A formula for computing the size (usually in bytes) of some span of fields.
/// Usually this will store a constant size, but it may reference the values of
/// fields only present at runtime.
#[derive(Clone, Debug)]
pub enum SizeExpression {
    /// Evaluates to the given constant size.
    Constant(usize),

    /// Defines be a field name path where the size can be found at runtime by
    /// evaluating the value located at the given path.
    ///
    /// The path is expected to point to a primitive field that will be cast to
    /// a usize.
    FieldLength(Vec<String>),

    /// Evaluated by summing up all inner expressions.
    Sum(Vec<SizeExpression>),

    /// Evaluated by multiplying all the inner expressions together.
    Product(Vec<SizeExpression>),
}

impl SizeExpression {
    pub fn to_constant(&self) -> Option<usize> {
        if let Self::Constant(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    pub fn add(self, other: SizeExpression) -> SizeExpression {
        let mut els = vec![];

        match self {
            SizeExpression::Sum(mut inner) => els.append(&mut inner),
            _ => els.push(self),
        };
        match other {
            SizeExpression::Sum(mut inner) => els.append(&mut inner),
            _ => els.push(other),
        };

        let mut const_sum = 0;
        els = els
            .into_iter()
            .filter_map(|el| {
                if let SizeExpression::Constant(v) = el {
                    const_sum += v;
                    None
                } else {
                    Some(el)
                }
            })
            .collect();

        if const_sum != 0 || els.is_empty() {
            els.push(Self::Constant(const_sum));
        }

        if els.len() == 1 {
            return els.into_iter().next().unwrap();
        }

        Self::Sum(els)
    }

    // TODO: Implement similarly to add()
    pub fn mul(self, other: SizeExpression) -> SizeExpression {
        if let SizeExpression::Constant(x) = &self {
            if let SizeExpression::Constant(y) = &other {
                return SizeExpression::Constant(*x * *y);
            }
        }

        let mut els = vec![];

        match self {
            SizeExpression::Product(mut inner) => els.append(&mut inner),
            _ => els.push(self),
        };
        match other {
            SizeExpression::Product(mut inner) => els.append(&mut inner),
            _ => els.push(other),
        };

        Self::Product(els)
    }

    pub fn scoped(self, field_name: &str) -> SizeExpression {
        match self {
            v @ Self::Constant(_) => v,
            Self::FieldLength(mut path) => {
                let mut new_path = vec![];
                new_path.push(field_name.to_string());
                new_path.append(&mut path);
                Self::FieldLength(new_path)
            }
            Self::Sum(inner) => {
                Self::Sum(inner.into_iter().map(|e| e.scoped(field_name)).collect())
            }
            Self::Product(inner) => {
                Self::Product(inner.into_iter().map(|e| e.scoped(field_name)).collect())
            }
        }
    }

    pub fn referenced_field_names<'a>(&'a self) -> HashSet<&'a str> {
        let mut out = HashSet::new();
        match self {
            Self::Constant(_) => {}
            Self::FieldLength(path) => {
                out.insert(path[0].as_str());
            }
            Self::Sum(inner) | Self::Product(inner) => {
                for e in inner {
                    let field_names = e.referenced_field_names();
                    out.extend(field_names.iter());
                }
            }
        }

        out
    }

    /// Compiles the expression to a string of Rust code that can be used in the
    /// parse() function of a struct to evaluate the value of this
    /// expression.
    ///
    /// TODO: Must check for overflows in the compiled expression.
    pub fn compile(&self, scope: &HashMap<&str, &Field>) -> String {
        match self {
            Self::Constant(v) => (*v).to_string(),
            Self::FieldLength(path) => {
                let mut expr = format!("{}_value", path[0]); // '_value' is the suffix used in the internal codegen.

                // TODO: Fix this so that is supports flags.
                // TODO: Implement this for inner fields as well.
                if !scope.get(path[0].as_str()).unwrap().presence().is_empty() {
                    expr.push_str(".unwrap_or(0)");
                }

                for field in &path[1..] {
                    expr.push_str(&format!(".{}", field));
                }

                // TODO: Need to be more cautious with this especially if it loses precision.
                format!("({} as usize)", expr)
            }
            // TODO: Validate that we never overload in these calculations.
            Self::Sum(inner) => {
                let terms = inner.iter().map(|e| e.compile(scope)).collect::<Vec<_>>();
                format!("({})", terms.join(" + "))
            }
            Self::Product(inner) => {
                // TODO: Sometimes we'll need to sum unique up many uniquely sizes elements.
                // (and some times it can be optimized away).
                let terms = inner.iter().map(|e| e.compile(scope)).collect::<Vec<_>>();
                format!("({})", terms.join(" * "))
            }
        }
    }
}
