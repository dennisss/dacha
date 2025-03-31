/*

Queries are 'SQL-like' strings like "(field.subfield = X OR number = ?)"


Final format is OR of AND'ed queries
*/

use std::collections::HashMap;

use base_error::*;
use parsing::*;
use protobuf::{reflection::Reflection, MessageReflection};

use crate::key_utils::prefix_key_range;
use crate::query::*;

// TODO: Need this to a proc macro so that we can pre-compile the fields and
// verify they exist before runtime.
#[macro_export]
macro_rules! query {
    ($db: expr, $t:ty, $e:expr $(, $v:expr)* ) => {{
        let mut builder = $crate::query_parser::QueryBuilder::create($e)?;
        $(
        builder.bind($v);
        )*

        use $crate::common::const_default::ConstDefault;
        type Message = <$t as $crate::table::ProtobufTableTag>::Message;

        let query = builder.build(&Message::DEFAULT)?;
        $db.query::<$t>(&query).await?
    }};
}

// TODO: Maybe rename since we have a differnet struct named RawQuery.
#[macro_export]
macro_rules! raw_query {
    ($t:ty, $e:expr $(, $v:expr)* ) => {{
        let mut builder = $crate::query_parser::QueryBuilder::create($e)?;
        $(
        builder.bind($v);
        )*

        use $crate::common::const_default::ConstDefault;
        type Message = <$t as $crate::table::ProtobufTableTag>::Message;

        let query = builder.build(&Message::DEFAULT)?;
        query
    }};
}

// TODO: Add a limit=1 clause.
#[macro_export]
macro_rules! query_one {
    ($db: expr, $t:ty, $e:expr $(, $v:expr)* ) => {{
        let mut results = $crate::query!($db, $t, $e $(, $v)*);
        results.pop()
    }};
}

pub struct QueryBuilder {
    raw: RawQuery,
    values: Vec<QueryValue>,
}

impl QueryBuilder {
    pub fn create(expr: &str) -> Result<Self> {
        let raw = RawQuery::parse(expr)?.to_dnf();
        Ok(Self {
            raw,
            values: vec![],
        })
    }

    pub fn bind<V: Into<QueryValue>>(&mut self, value: V) -> &mut Self {
        self.values.push(value.into());
        self
    }

    pub fn build(&mut self, typ: &dyn MessageReflection) -> Result<Query> {
        let mut placeholders = vec![];
        Self::find_placeholder(&self.raw, &mut placeholders);
        placeholders.sort();
        placeholders.reverse();

        // Since some may be duplicated from DNF conversion.
        placeholders.dedup();

        if placeholders.len() != self.values.len() {
            return Err(err_msg(
                "Wrong number of values bound for placeholders in query",
            ));
        }

        let mut placeholder_map = HashMap::new();
        for i in 0..placeholders.len() {
            placeholder_map.insert(placeholders[i], self.values[i].clone());
        }

        let or_args = match &self.raw {
            RawQuery::Op(RawOp::Or, args) => &args[..],
            _ => core::slice::from_ref(&self.raw),
        };

        let mut out = Query::default();

        for and_op in or_args {
            let mut and = QueryAllOf::default();

            let and_args = match and_op {
                RawQuery::Op(RawOp::And, args) => &args[..],
                _ => core::slice::from_ref(and_op),
            };

            let mut skip = false;
            for q in and_args {
                match q {
                    RawQuery::Op(op, args) => {
                        Self::compile_binary_op(op, &args[..], typ, &placeholder_map, &mut and)?;
                    }
                    RawQuery::FuncCall(fname, args) => {
                        if fname.parts.len() != 1 {
                            return Err(err_msg("Function name can only have one part"));
                        }

                        let fname = fname.parts[0].to_ascii_lowercase();

                        if fname == "starts_with" {
                            Self::compile_starts_with(&args[..], typ, &placeholder_map, &mut and)?;
                        } else {
                            return Err(err_msg("Unsupported function call"));
                        }
                    }
                    RawQuery::BoolLiteral(v) => {
                        if !*v {
                            skip = true;
                            break;
                        }
                    }
                    _ => {
                        return Err(format_err!(
                            "'{:?}' is an unsupported primitize operation",
                            q
                        ))
                    }
                }
            }

            if skip {
                continue;
            }

            out.or(and);
        }

        Ok(out)
    }

    fn compile_binary_op(
        op: &RawOp,
        args: &[RawQuery],
        typ: &dyn MessageReflection,
        placeholder_map: &HashMap<usize, QueryValue>,
        out: &mut QueryAllOf,
    ) -> Result<()> {
        // TODO: Normalize idents to be on the left hand side of binary ops

        if args.len() != 2 {
            return Err(err_msg("Only ops with 2 arguments are supported"));
        }

        let ident = match &args[0] {
            RawQuery::Ident(v) => v,
            _ => return Err(err_msg("Expected ident")),
        };

        let raw_value = &args[1];

        let mut num_path = vec![];

        let mut r = Reflection::Message(typ);

        for name in &ident.parts {
            let msg = match r {
                Reflection::Message(m) => m,
                _ => return Err(err_msg("Can only get a field in a message")),
            };

            let num = msg
                .field_number_by_name(name.as_str())
                .ok_or_else(|| format_err!("No field named: {}", name))?;
            num_path.push(num);

            r = msg.field_by_number(num).unwrap();
        }

        let value = match raw_value {
            RawQuery::Placeholder(v) => {
                // TODO: Cast this to a reasonable type for the field based on 'r'.
                placeholder_map.get(v).unwrap().clone()
            }
            RawQuery::BoolLiteral(v) => QueryValue::Bool(*v),
            _ => return Err(format_err!("Unsupported query value type: {:?}", raw_value)),
        };

        let op = match op {
            RawOp::And | RawOp::Or => return Err(err_msg("Primitive AND/OR not supported")),
            RawOp::Eq => QueryOp::Eq,
            RawOp::LessThan => QueryOp::LessThan,
            RawOp::LessThanOrEqual => QueryOp::LessThanOrEqual,
            RawOp::GreaterThan => QueryOp::GreaterThan,
            RawOp::GreaterThanOrEqual => QueryOp::GreaterThanOrEqual,
        };

        out.and(&num_path, QueryComparison { op, rhs: value });

        Ok(())
    }

    fn compile_starts_with(
        args: &[RawQuery],
        typ: &dyn MessageReflection,
        placeholder_map: &HashMap<usize, QueryValue>,
        out: &mut QueryAllOf,
    ) -> Result<()> {
        if args.len() != 2 {
            return Err(err_msg("Only ops with 2 arguments are supported"));
        }

        let ident = match &args[0] {
            RawQuery::Ident(v) => v,
            _ => return Err(err_msg("Expected ident")),
        };

        let placeholder = match &args[1] {
            RawQuery::Placeholder(v) => *v,
            _ => return Err(err_msg("Expected placeholder as value")),
        };

        // TODO: Deduplicate this logic.
        let mut num_path = vec![];

        let mut r = Reflection::Message(typ);

        for name in &ident.parts {
            let msg = match r {
                Reflection::Message(m) => m,
                _ => return Err(err_msg("Can only get a field in a message")),
            };

            let num = msg
                .field_number_by_name(name.as_str())
                .ok_or_else(|| format_err!("No field named: {}", name))?;
            num_path.push(num);

            r = msg.field_by_number(num).unwrap();
        }

        // TODO: Cast this to a reasonable type for the field based on 'r'.
        let value = placeholder_map.get(&placeholder).unwrap().clone();

        let value_bytes = match value.reflect() {
            Reflection::String(s) => s.as_bytes(),
            Reflection::Bytes(v) => v,
            _ => {
                return Err(err_msg(
                    "STARTS_WITH function only supported for string/byte types",
                ))
            }
        };

        let (start, end) = prefix_key_range(value_bytes);

        out.and(
            &num_path,
            QueryComparison {
                op: QueryOp::GreaterThanOrEqual,
                rhs: QueryValue::Bytes(start.to_vec()),
            },
        );
        out.and(
            &num_path,
            QueryComparison {
                op: QueryOp::LessThan,
                rhs: QueryValue::Bytes(end.to_vec()),
            },
        );

        Ok(())
    }

    fn find_placeholder(query: &RawQuery, out: &mut Vec<usize>) {
        match query {
            RawQuery::Op(_, args) | RawQuery::FuncCall(_, args) => {
                for arg in args {
                    Self::find_placeholder(arg, out);
                }
            }
            RawQuery::Placeholder(pos) => {
                out.push(*pos);
            }
            RawQuery::Ident(_) | RawQuery::BoolLiteral(_) => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum RawQuery {
    Op(RawOp, Vec<RawQuery>),

    /// Name of a function to call and the arguments to the function.
    FuncCall(IdentPath, Vec<RawQuery>),

    /// The number is the position of the placeholder (in bytes away from the
    /// END of the query string).
    Placeholder(usize),

    BoolLiteral(bool),

    Ident(IdentPath),
}

// TODO: Move to the parsing crate.
pub fn peek2<I: Clone, T, P: Parser<T, I>>(p: P) -> impl Parser<T, I> {
    move |input: I| {
        if let Ok((v, _)) = p(input.clone()) {
            Ok((v, input))
        } else {
            // TODO: Return the parser error?
            Err(err_msg("Peek failed"))
        }
    }
}

impl RawQuery {
    fn parse(input: &str) -> Result<Self> {
        let (v, _) = complete(|i| Self::parse_op(i, 0))(input)?;
        Ok(v)
    }

    fn parse_op<'a>(input: &'a str, precedence: usize) -> ParseResult<Self, &'a str> {
        seq!(c => {

            let mut expr = c.next(Self::parse_inner)?;

            loop {
                let v: Option<RawOp> = c.next(opt(peek2(RawOp::parse)))?;
                let op = match v {
                    Some(op) => {
                        if op as usize <= precedence {
                            break;
                        }

                        op
                    }
                    None => break
                };

                // Consume the op we just peeked.
                c.next(RawOp::parse)?;

                let rhs = c.next(|i| Self::parse_op(i, op as usize))?;
                expr = Self::Op(op, vec![expr, rhs]);
            }


            Ok(expr)
        })(input)
    }

    // Parser without binary ops.
    parser!(parse_inner<&str, Self> => alt!(
        seq!(c => {
            c.next(is(symbol, '('))?;
            let inner = c.next(|i| Self::parse_op(i, 0))?;
            c.next(is(symbol, ')'))?;
            Ok(inner)
        }),

        seq!(c => {
            c.next(skip_to)?;

            let word = c.next(take_while1(|c: char| !c.is_whitespace()))?.to_ascii_lowercase();

            Ok(match word.as_str() {
                "true" => Self::BoolLiteral(true),
                "false" => Self::BoolLiteral(false),
                _ => return Err(err_msg("Not a bool literal"))
            })
        }),

        seq!(c => {
            let fname = c.next(IdentPath::parse)?;
            c.next(is(symbol, '('))?;

            let args = c.next(delimited(|i| Self::parse_op(i, 0), is(symbol, ',')))?;

            c.next(is(symbol, ')'))?;

            Ok(Self::FuncCall(fname, args))
        }),

        map(IdentPath::parse, |v| Self::Ident(v)),

        map(placeholder, |v| Self::Placeholder(v))

    ));

    /// Converts the query to disjunctive normal form (an OR of AND'ed simple
    /// expressions).
    ///
    /// - First this flattens AND/OR ops that are children of AND/OR ops into
    ///   their parents.
    /// - Then when we see an AND op that has an OR op as a child, we invert the
    ///   expression at that node.
    /// - The process continues from the inner to outer most node.
    ///
    /// NOTE: This currently assumes that there are no AND/OR operations inside
    /// of regular ops like '>', '=', etc.
    fn to_dnf(self) -> Self {
        match self {
            Self::Op(op, args) => {
                let mut new_args = vec![];

                // Recursive to_dnf and flattening
                for arg in args {
                    let mut arg = arg.to_dnf();

                    if op == RawOp::And || op == RawOp::Or {
                        if let RawQuery::Op(inner_op, inner_args) = &mut arg {
                            if *inner_op == op {
                                for inner_arg in inner_args.drain(..) {
                                    new_args.push(inner_arg);
                                }
                                continue;
                            }
                        }
                    }

                    new_args.push(arg);
                }

                if op == RawOp::And {
                    let mut or_index = None;

                    for (i, arg) in new_args.iter().enumerate() {
                        if let RawQuery::Op(inner_op, _) = arg {
                            if *inner_op == RawOp::Or {
                                or_index = Some(i);
                                break;
                            }
                        }
                    }

                    if let Some(i) = or_index {
                        // Need to replace the current AND op with an OR op

                        let old_or_args = match new_args.swap_remove(i) {
                            RawQuery::Op(_, args) => args,
                            _ => panic!(),
                        };

                        let mut new_or_args = vec![];
                        for part in old_or_args {
                            let mut new_and_args = new_args.clone();
                            new_and_args.push(part);
                            new_or_args.push(Self::Op(RawOp::And, new_and_args));
                        }

                        // The to_dnf() is to handle the case in there are potentially multiple OR
                        // ops that were nested below the original AND op. This is somewhat
                        // in-efficient though.
                        return Self::Op(RawOp::Or, new_or_args).to_dnf();
                    }
                }

                Self::Op(op, new_args)
            }
            Self::Ident(_) | Self::Placeholder(_) | Self::FuncCall(_, _) | Self::BoolLiteral(_) => {
                self
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct IdentPath {
    parts: Vec<String>,
}

impl IdentPath {
    parser!(parse<&str, Self> => seq!(c => {
        c.next(skip_to)?;
        let parts = c.next(delimited1(protobuf::tokenizer::ident, tag(".")))?
            .iter().map(|v| v.to_string()).collect();
        Ok(Self {
            parts
        })
    }));
}

#[derive(Copy, Debug, Clone, PartialEq, Eq)]
enum RawOp {
    // NOTE: The above code assumes 0 is < all ops.
    And = 1,
    Or = 2,
    Eq = 3,
    LessThan = 4,
    LessThanOrEqual = 5,
    GreaterThan = 6,
    GreaterThanOrEqual = 7,
}

fn placeholder(input: &str) -> ParseResult<usize, &str> {
    let pos = input.len();
    is(symbol, '?')(input).map(|(_, rest)| (pos, rest))
}

impl RawOp {
    parser!(parse<&str, Self> => seq!(c => {
        c.next(skip_to)?;

        let word = c.next(take_while1(|c: char| !c.is_whitespace()))?.to_ascii_lowercase();

        Ok(match word.as_str() {
            ">" => Self::GreaterThan,
            ">=" => Self::GreaterThanOrEqual,
            "<" => Self::LessThan,
            "<=" => Self::LessThanOrEqual,
            "=" => Self::Eq,
            "and" => Self::And,
            "or" => Self::Or,
            _ => return Err(err_msg("Not a op"))
        })
    }));

    fn reverse_order(&self) -> Self {
        match self {
            RawOp::And | RawOp::Or | RawOp::Eq => *self,
            RawOp::LessThan => RawOp::GreaterThan,
            RawOp::LessThanOrEqual => RawOp::GreaterThanOrEqual,
            RawOp::GreaterThan => RawOp::LessThan,
            RawOp::GreaterThanOrEqual => RawOp::LessThanOrEqual,
        }
    }
}

//

// Helper to skip non-token whitespace.
parser!(skip_to<&str, ()> => {
    map(take_while(|c: char| c.is_whitespace()), |_| ())
});

parser!(symbol<&str, char> => seq!(c => {
    c.next(skip_to)?;
    c.next(one_of(".<>=()?,"))
}));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parser_test() {
        /*
        println!("{:?}", RawQuery::parse("field").unwrap());
        println!("{:?}", RawQuery::parse("?").unwrap());
        println!("{:#?}", RawQuery::parse("name = ?").unwrap());
        println!("{:#?}", RawQuery::parse("name = ? AND other >= ?").unwrap());
        println!(
            "{:#?}",
            RawQuery::parse("(name = ? AND other >= ?) OR b = ?").unwrap()
        );
        println!("{:#?}", RawQuery::parse("field.subfield < ?").unwrap());
        */

        println!(
            "{:#?}",
            RawQuery::parse("(a = ? OR b >= ?) AND C = ?")
                .unwrap()
                .to_dnf()
        );

        println!("{:#?}", RawQuery::parse("STARTS_WITH(A, ?)").unwrap());

        assert!(RawQuery::parse("").is_err());

        assert_eq!(
            RawQuery::parse("TRUE").unwrap(),
            RawQuery::BoolLiteral(true)
        );
        assert_eq!(
            RawQuery::parse("FALSE").unwrap(),
            RawQuery::BoolLiteral(false)
        );
    }
}
