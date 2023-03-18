use common::errors::*;
use parsing::*;
use parsing::{ParseCursor, ParseError};
use protobuf::tokenizer::{float_lit, ident, int_lit, strLit};

/*

The main tokenization challenge with this is:
- Its not safe to consume whitespace BEFORE a token because some rules handle indnetation tracking
- But, we can always consume all inline whitespace after a token.
- Additionally, after a token, if we are in a parens or square brackets rule, we can additionally consume new lines.

*/

/// TODO: Use a perfect hash table for this?
///
/// NOTE: An identifier is not allowed to be any of these words.
const KEYWORDS: &'static [&'static str] = &[
    "await", "else", "import", "pass", "break", "except", "in", "raise", "class", "finally", "is",
    "return", "and", "continue", "for", "lambda", "try", "as", "def", "from", "nonlocal", "while",
    "assert", "del", "global", "not", "with", "async", "elif", "if", "or", "yield", "False",
    "True", "None",
];

// TODO: Always consume any inline white space that is following a non-white
// space token as this is also allowable (only an issue for space before them.)

fn keyword<'a>(name: &'static str) -> impl Fn(&'a str) -> Result<((), &'a str)> {
    seq!(c => {
        let v = c.next(protobuf::tokenizer::ident)?;
        if v != name {
            return Err(err_msg("Wrong keyword"));
        }

        Ok(())
    })
}

// fn operator()

parser!(identifier<&str, String> => seq!(c => {
    let v = c.next(protobuf::tokenizer::ident)?;
    if KEYWORDS.contains(&v.as_str()) {
        return Err(err_msg("Unexpected keyword"));
    }

    Ok(v)
}));

/// TODO: Mixing tabs and spaces is disallowed in Python
#[derive(Clone)]
pub struct InlineWhitespace {
    pub num_tabs: usize,
    pub num_spaces: usize,
}

impl InlineWhitespace {
    pub fn parse(input: &str) -> Result<(Self, &str)> {
        let mut num_spaces = 0;
        let mut num_tabs = 0;

        let mut last_index = input.len();
        for (i, c) in input.char_indices() {
            if c == ' ' {
                num_spaces += 1;
            } else if c == '\t' {
                num_tabs += 1;
            } else {
                last_index = i;
                break;
            }
        }

        Ok((
            Self {
                num_spaces,
                num_tabs,
            },
            input.split_at(last_index).1,
        ))
    }
}

parser!(any_whitespace<&str, ()> => {
    seq!(c => {
        c.next(many(empty_line))?;

        c.next(InlineWhitespace::parse)?;

        Ok(())
    })
    // map(parsing::take_while(|v| {
    //     v == ' ' || v == '\t' || v == '\n' || v == '\r'
    // }), |_| ())
});

#[derive(Clone)]
pub struct ParsingContext {
    /// Amount of indentation prefixing the current suite of statements. A suite
    /// should only continue parsing lines if lines have an indentation >= this
    /// amount.
    indent: InlineWhitespace,

    /// Whether or not we aer currently parsing inside of parenthesis.
    /// This is mainly used to determine if we are allowed to consume new lines
    in_parens: bool,

    operator_priority: usize,
}

impl Default for ParsingContext {
    fn default() -> Self {
        Self {
            indent: InlineWhitespace {
                num_tabs: 0,
                num_spaces: 0,
            },
            in_parens: false,
            operator_priority: DEFAULT_OP_PRIORITY,
        }
    }
}

impl ParsingContext {
    fn consume_whitespace<'a>(&self, input: &'a str) -> Result<((), &'a str)> {
        if self.in_parens {
            any_whitespace(input)
        } else {
            map(InlineWhitespace::parse, |v| ())(input)
        }
    }
}

parser!(newline<&str, ()> => alt!(
    map(tag("\r\n"), |_| ()),
    map(tag("\n"), |_| ()),
    map(tag("\r"), |_| ())
));

#[derive(Clone, Debug)]
pub struct File {
    pub statements: Vec<Statement>,
}

impl File {
    // TODO: Also allow any lines with completely whitespace or containing a
    // comment. ^ This could probably re-use some of the suite code.

    // File = {Statement | newline} eof .

    pub fn parse(mut input: &str) -> Result<Self> {
        let mut statements = vec![];

        let context = ParsingContext::default();

        while !input.is_empty() {
            if parse_next!(input, opt(empty_line)).is_some() {
                continue;
            }

            statements.extend(parse_next!(input, |v| Statement::parse(v, &context)).into_iter());
        }

        Ok(Self { statements })
    }
}

// Whitespace following by optionally a comment and ending in a \n or EOF.
// TODO: Replace all usages of newline with this.
fn empty_line<'a>(mut input: &'a str) -> Result<((), &'a str)> {
    parse_next!(input, InlineWhitespace::parse);

    if parse_next!(input, opt(tag("#"))).is_some() {
        parse_next!(input, take_while(|c| c != '\r' && c != '\n'));
    }

    if !input.is_empty() {
        parse_next!(input, newline);
    }

    Ok(((), input))
}

#[derive(Clone, Debug)]
pub enum Statement {
    Def(DefStatement),
    // If(IfStatement),
    // For(ForStatement),
    Return(Option<Expression>),
    Break,
    Continue,
    Pass,
    Assign {
        target: Expression,
        op: BinaryOp,
        value: Expression,
    },
    Expression(Expression),
}

impl Statement {
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Vec<Self>, &'a str)> {
        simple_statement(input, context)
    }

    // Statement = DefStmt | IfStmt | ForStmt | SimpleStmt .
    // parser!(parse<&str, Self> => alt!(
    //     map(DefStatement::parse, |v| Self::Def(v)),
    //     map(IfStatement::parse, |v| Self::If(v)),
    //     map(ForStatement::parse, |v| Self::For(v)),
    //     simple_statement
    // ));
}

#[derive(Clone, Debug)]
pub struct DefStatement {
    pub name: String,
    pub params: Vec<Parameter>,
    pub body: Vec<Statement>,
}

impl DefStatement {
    /*
    // DefStmt = 'def' identifier '(' [Parameters [',']] ')' ':' Suite .
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        parse_next!(input, keyword("def"));
        parse_next!(input, InlineWhitespace::parse);

        let name = parse_next!(input, identifier);
        parse_next!(input, InlineWhitespace::parse);

        parse_next!(input, tag("("));
        // TODO: Consume whitespace after paren (with newlines)
        let params = {
            let mut inner_context = context.clone();
            inner_context.in_parens = true;

            parse_next!(input, Parameter::parse_set, &inner_context)
        };

        parse_next!(input, tag(")"));
        parse_next!(input, InlineWhitespace::parse);

        parse_next!(input, tag(":"));
        parse_next!(input, InlineWhitespace::parse); // TOOD: Check this

        // TODO: Suite

        Ok((
            Self {
                name,
                params,
                body: todo!(),
            },
            input,
        ))
    }
    */
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub form: ParameterForm,
}

#[derive(Clone, Debug)]
pub enum ParameterForm {
    Required,
    Optional { default_value: Test },
    Variadic,
    KeywordArgs,
}

impl Parameter {
    /// Parses zero or more parameters. If the parameters appear if a group of
    /// parenthesis, then this will also accept a trailing comma.
    ///
    /// This parser also consumes any leading or trailing whitespace using
    /// ParsingContext::consume_whitespace.
    ///
    /// Loose Grammar Rule:
    ///   Parameters = Parameter {',' Parameter}.
    fn parse_set<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Vec<Self>, &'a str)> {
        let mut out = vec![];

        parse_next!(input, |v| context.consume_whitespace(v));

        let mut extra_comma = false;
        while let Ok((param, rest)) = Self::parse(input, context) {
            input = rest;
            out.push(param);
            extra_comma = false;

            parse_next!(input, |v| context.consume_whitespace(v));

            if parse_next!(input, opt(tag(","))).is_some() {
                extra_comma = true;
            } else {
                break;
            }

            parse_next!(input, |v| context.consume_whitespace(v));
        }

        if extra_comma && !context.in_parens {
            return Err(err_msg("Expected additional parameter"));
        }

        Ok((out, input))
    }

    /// Grammar rule:
    ///   Parameter = identifier | identifier '=' Test | '*' identifier | '**'
    ///   identifier .
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        if parse_next!(input, opt(tag("**"))).is_some() {
            let name = parse_next!(input, identifier);
            Ok((
                Self {
                    name,
                    form: ParameterForm::KeywordArgs,
                },
                input,
            ))
        } else if parse_next!(input, opt(tag("*"))).is_some() {
            let name = parse_next!(input, identifier);
            Ok((
                Self {
                    name,
                    form: ParameterForm::Variadic,
                },
                input,
            ))
        } else {
            let name = parse_next!(input, identifier);

            if parse_next!(input, opt(tag("="))).is_some() {
                let default_value = parse_next!(input, Test::parse, context);
                Ok((
                    Self {
                        name,
                        form: ParameterForm::Optional { default_value },
                    },
                    input,
                ))
            } else {
                Ok((
                    Self {
                        name,
                        form: ParameterForm::Required,
                    },
                    input,
                ))
            }
        }
    }
}

/*
IfStmt = 'if' Test ':' Suite {'elif' Test ':' Suite} ['else' ':' Suite] .

ForStmt = 'for' LoopVariables 'in' Expression ':' Suite .
*/

/// Grammar Rule:
///   Suite = [newline indent {Statement} outdent] | SimpleStmt .
fn suite<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Vec<Statement>, &'a str)> {
    let mut out = vec![];

    if parse_next!(input, opt(newline)).is_some() {
        //

        // TODO: Must skip any empty lines here.
    } else {
        //
    }

    Ok((out, input))
}

// NOTE: Unless there are inner parens, a simple statement is only composed of a
// single line and will not have other statements inside of it.
/*
SimpleStmt = SmallStmt {';' SmallStmt} [';'] '\n' .
# NOTE: '\n' optional at EOF
*/
fn simple_statement<'a>(
    mut input: &'a str,
    context: &ParsingContext,
) -> Result<(Vec<Statement>, &'a str)> {
    let mut out = vec![];

    out.push(parse_next!(input, small_statement, context));

    while parse_next!(input, opt(tag(";"))).is_some() {
        parse_next!(input, |v| context.consume_whitespace(v));

        if let Some(v) = parse_next!(input, opt(|v| small_statement(v, context))) {
            out.push(v);
        } else {
            break;
        }
    }

    if !input.is_empty() {
        parse_next!(input, newline);
    }

    Ok((out, input))
}

/*
SmallStmt = ReturnStmt
          | BreakStmt | ContinueStmt | PassStmt
          | AssignStmt
          | ExprStmt
          | LoadStmt
          .

ReturnStmt   = 'return' [Expression] .
BreakStmt    = 'break' .
ContinueStmt = 'continue' .
PassStmt     = 'pass' .
AssignStmt   = Expression ('=' | '+=' | '-=' | '*=' | '/=' | '//=' | '%=' | '&=' | '|=' | '^=' | '<<=' | '>>=') Expression .
ExprStmt     = Expression .

LoadStmt = 'load' '(' string {',' [identifier '='] string} [','] ')' .

*/
fn small_statement<'a>(
    mut input: &'a str,
    context: &ParsingContext,
) -> Result<(Statement, &'a str)> {
    // TODO: Allow whitespace after these?
    // TODO: Ensure that the expression after the return is optional
    alt!(
        seq!(c => {
            c.next(tag("return"))?;
            c.next(InlineWhitespace::parse)?;

            let value = c.next(opt(|v| Expression::parse(v, context)))?;
            Ok(Statement::Return(value))
        }),
        map(keyword("break"), |_| Statement::Break),
        map(keyword("continue"), |_| Statement::Continue),
        map(keyword("pass"), |_| Statement::Pass),
        // TODO: Assign
        seq!(c => {
            let expr = c.next(|v| Expression::parse(v, context))?;

            if let Some(Operator::Assign(op)) = c.next(opt(|v| Operator::parse(v, context)))? {
                let e2 = c.next(|v| Expression::parse(v, context))?;
                Ok(Statement::Assign {
                    target: expr,
                    op,
                    value: e2
                })
            } else {
                Ok(Statement::Expression(expr))
            }
        }) // TODO: Implement 'load' statements.
    )(input)
}

#[derive(Clone, Debug)]
pub enum Test {
    If(Box<IfExpression>),
    Primary(PrimaryExpression),
    Unary(Box<UnaryExpression>),
    Binary(Box<BinaryExpression>),
}

impl Test {
    // Test = IfExpr | PrimaryExpr | UnaryExpr | BinaryExpr | LambdaExpr .
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        // Parse LambdaExpr rule.
        if parse_next!(input, opt(keyword("lambda"))).is_some() {
            return Err(err_msg("Lambda not supported"));
        }

        let mut value = {
            if let Some(v) = parse_next!(input, opt(|v| UnaryExpression::parse(v, context))) {
                Self::Unary(Box::new(v))
            } else if let Some(v) =
                parse_next!(input, opt(|v| PrimaryExpression::parse(v, context)))
            {
                Self::Primary(v)
            } else {
                return Err(err_msg("Invalid test"));
            }
        };

        while let Ok((Operator::Binary(op), rest)) = Operator::parse(input, context) {
            if (op as usize) < context.operator_priority {
                break;
            }

            input = rest;
            parse_next!(input, |v| context.consume_whitespace(v));

            let mut inner_context = context.clone();
            inner_context.operator_priority = op as usize;

            let right = parse_next!(input, Test::parse, &inner_context);

            value = Self::Binary(Box::new(BinaryExpression {
                op,
                left: value,
                right,
            }));
        }

        if IF_OP_PRIORITY >= context.operator_priority {
            if parse_next!(input, opt(keyword("if"))).is_some() {
                parse_next!(input, |v| context.consume_whitespace(v));

                // TODO: Adjust the priority in the context.
                let condition = parse_next!(input, Test::parse, context);

                parse_next!(input, keyword("else"));
                parse_next!(input, |v| context.consume_whitespace(v));

                let false_value = parse_next!(input, Test::parse, context);

                value = Self::If(Box::new(IfExpression {
                    condition,
                    true_value: value,
                    false_value,
                }));
            }
        }

        Ok((value, input))
    }
}

/*
IfExpr = Test 'if' Test 'else' Test .
*/
#[derive(Clone, Debug)]
pub struct IfExpression {
    pub condition: Test,
    pub true_value: Test,
    pub false_value: Test,
}

/*
PrimaryExpr = Operand
            | PrimaryExpr DotSuffix
            | PrimaryExpr CallSuffix
            | PrimaryExpr SliceSuffix
            .

DotSuffix   = '.' identifier .
SliceSuffix = '[' [Expression] ':' [Test] [':' [Test]] ']'
            | '[' Expression ']'
            .
CallSuffix  = '(' [Arguments [',']] ')' .


*/
#[derive(Clone, Debug)]
pub struct PrimaryExpression {
    pub base: Operand,
    pub suffixes: Vec<PrimaryExpressionSuffix>,
}

impl PrimaryExpression {
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        let base = parse_next!(input, Operand::parse, context);
        let suffixes = parse_next!(input, many(|v| PrimaryExpressionSuffix::parse(v, context)));
        Ok((Self { base, suffixes }, input))
    }
}

#[derive(Clone, Debug)]
pub enum SliceIndex {
    Single(Expression),
    Range {
        /// If none, then start at the beginning of the collection.
        start: Option<Expression>,
        interval: Option<Test>,
        /// If None, go to the end of the collection.
        end: Option<Test>,
    },
}

#[derive(Clone, Debug)]
pub enum PrimaryExpressionSuffix {
    Dot(String),
    Slice(SliceIndex),
    Call(Vec<Argument>),
}

impl PrimaryExpressionSuffix {
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        alt!(
            seq!(c => {
                c.next(tag("."))?;
                c.next(|v| context.consume_whitespace(v))?;

                let ident = c.next(identifier)?;
                c.next(|v| context.consume_whitespace(v))?;

                Ok(Self::Dot(ident))
            }),
            seq!(c => {
                c.next(tag("("))?;

                let mut inner_context = context.clone();
                inner_context.in_parens = true;

                c.next(|v| inner_context.consume_whitespace(v))?;

                let args = c.next(|v| Argument::parse_many(v, &inner_context))?;

                c.next(tag(")"))?;
                c.next(|v| context.consume_whitespace(v))?;

                Ok(Self::Call(args))
            }),
            seq!(c => {
                c.next(tag("["))?;

                let mut inner_context = context.clone();
                inner_context.in_parens = true;

                c.next(|v| inner_context.consume_whitespace(v))?;

                let first = c.next(opt(|v| Expression::parse(v, context)))?;

                let is_span = c.next(opt(tag(":")))?.is_some();
                c.next(|v| context.consume_whitespace(v))?;

                let second = if is_span {
                    c.next(opt(|v| Test::parse(v, context)))?
                } else {
                    None
                };

                let is_three_parts = if is_span {
                    let v = c.next(opt(tag(":")))?.is_some();
                    c.next(|v| context.consume_whitespace(v))?;
                    v
                } else { false };

                let third = if is_three_parts {
                    c.next(opt(|v| Test::parse(v, context)))?
                } else {
                    None
                };

                c.next(tag("]"))?;
                c.next(|v| context.consume_whitespace(v))?;

                if !is_span {
                    let index = first.ok_or_else(|| err_msg("Empty index"))?;
                    Ok(Self::Slice(SliceIndex::Single(index)))
                } else if !is_three_parts {
                    Ok(Self::Slice(SliceIndex::Range { start: first, interval: None, end: second }))
                } else {
                    Ok(Self::Slice(SliceIndex::Range { start: first, interval: second, end: third }))
                }
            })
        )(input)
    }
}

/*
Arguments = Argument {',' Argument} .
Argument  = Test | identifier '=' Test | '*' Test | '**' Test .
*/
#[derive(Clone, Debug)]

pub enum Argument {
    Value(Test),
    KeyValue(String, Test),
    Variadic(Test),
    KeywordArgs(Test),
}

impl Argument {
    fn parse_many<'a>(
        mut input: &'a str,
        context: &ParsingContext,
    ) -> Result<(Vec<Self>, &'a str)> {
        seq!(c => {

            let vals = c.next(delimited(|v| Self::parse(v, context), seq!(c => {
                c.next(tag(","))?;
                c.next(|v| context.consume_whitespace(v))?;
                Ok(())
            })))?;

            if !vals.is_empty() {
               c.next(opt(seq!(c => {
                    c.next(tag(","))?;
                    c.next(|v| context.consume_whitespace(v))?;
                    Ok(())
                })))?;
            }

            Ok(vals)


        })(input)
    }

    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        alt!(
            seq!(c => {
                c.next(tag("**"))?;
                c.next(|v| context.consume_whitespace(v))?;
                let v = c.next(|v| Test::parse(v, context))?;
                Ok(Self::KeywordArgs(v))
            }),
            seq!(c => {
                c.next(tag("*"))?;
                c.next(|v| context.consume_whitespace(v))?;
                let v = c.next(|v| Test::parse(v, context))?;
                Ok(Self::Variadic(v))
            }),
            seq!(c => {
                let name = c.next(identifier)?;
                c.next(|v| context.consume_whitespace(v))?;

                c.next(tag("="))?;
                c.next(|v| context.consume_whitespace(v))?;

                let value = c.next(|v| Test::parse(v, context))?;
                Ok(Self::KeyValue(name, value))
            }),
            seq!(c => {
                let v = c.next(|v| Test::parse(v, context))?;
                Ok(Self::Value(v))
            })
        )(input)
    }
}

/*
UnaryExpr = '+' Test
          | '-' Test
          | '~' Test
          | 'not' Test
          .
*/
#[derive(Clone, Debug)]
pub struct UnaryExpression {
    pub op: UnaryOp,
    pub value: Test,
}

#[derive(Clone, Debug)]
pub enum UnaryOp {
    Plus,
    Negation,
    BitwiseNegation,
    Not,
}

impl UnaryExpression {
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        // TODO: Implement this in terms of Operator to ensure that the entire operator
        // is consumed.

        let op = parse_next!(
            input,
            alt!(
                map(tag("+"), |_| UnaryOp::Plus),
                map(tag("-"), |_| UnaryOp::Negation),
                map(tag("~"), |_| UnaryOp::BitwiseNegation),
                map(keyword("not"), |_| UnaryOp::Not)
            )
        );
        parse_next!(input, |v| context.consume_whitespace(v));

        let mut inner_context = context.clone();
        inner_context.operator_priority = UNARY_OP_PRIORITY;

        let value = parse_next!(input, Test::parse, context);

        Ok((Self { op, value }, input))
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Operator {
    Assign(BinaryOp),
    Binary(BinaryOp),
}

impl Operator {
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        // NOTE: When there are multiple symbols with the same starting characters, they
        // are sorted by descending length.

        let op = parse_next!(
            input,
            alt!(
                map(tag("=="), |_| Self::Binary(BinaryOp::IsEqual)),
                map(tag("="), |_| Self::Assign(BinaryOp::IsEqual)),
                map(tag("!="), |_| Self::Binary(BinaryOp::NotEqual)),
                map(tag("<<="), |_| Self::Assign(BinaryOp::ShiftLeft)),
                map(tag("<<"), |_| Self::Binary(BinaryOp::ShiftLeft)),
                map(tag("<="), |_| Self::Binary(BinaryOp::LessEqual)),
                map(tag("<"), |_| Self::Binary(BinaryOp::LessThan)),
                map(tag(">>="), |_| Self::Assign(BinaryOp::ShiftRight)),
                map(tag(">>"), |_| Self::Binary(BinaryOp::ShiftRight)),
                map(tag(">="), |_| Self::Binary(BinaryOp::GreaterEqual)),
                map(tag(">"), |_| Self::Binary(BinaryOp::GreaterThan)),
                map(keyword("in"), |_| Self::Binary(BinaryOp::In)),
                map(
                    seq!(c => {
                        c.next(keyword("not"))?;
                        c.next(keyword("in"))?;
                        Ok(())
                    }),
                    |_| Self::Binary(BinaryOp::NotIn)
                ),
                map(tag("|="), |_| Self::Assign(BinaryOp::BitOr)),
                map(tag("|"), |_| Self::Binary(BinaryOp::BitOr)),
                map(tag("^="), |_| Self::Assign(BinaryOp::Xor)),
                map(tag("^"), |_| Self::Binary(BinaryOp::Xor)),
                map(tag("&="), |_| Self::Assign(BinaryOp::BitAnd)),
                map(tag("&"), |_| Self::Binary(BinaryOp::BitAnd)),
                map(tag("-="), |_| Self::Assign(BinaryOp::Subtract)),
                map(tag("-"), |_| Self::Binary(BinaryOp::Subtract)),
                map(tag("+="), |_| Self::Assign(BinaryOp::Add)),
                map(tag("+"), |_| Self::Binary(BinaryOp::Add)),
                map(tag("*="), |_| Self::Assign(BinaryOp::Multiply)),
                map(tag("*"), |_| Self::Binary(BinaryOp::Multiply)),
                map(tag("%="), |_| Self::Assign(BinaryOp::Modulus)),
                map(tag("%"), |_| Self::Binary(BinaryOp::Modulus)),
                map(tag("//="), |_| Self::Assign(BinaryOp::FloorDivide)),
                map(tag("//"), |_| Self::Binary(BinaryOp::FloorDivide)),
                map(tag("/="), |_| Self::Assign(BinaryOp::TrueDivide)),
                map(tag("/"), |_| Self::Binary(BinaryOp::TrueDivide))
            )
        );

        // Must be followed by

        parse_next!(input, |v| context.consume_whitespace(v));

        Ok((op, input))
    }
}

/*

BinaryExpr = Test {Binop Test} .

Binop = 'or'
      | 'and'
      | '==' | '!=' | '<' | '>' | '<=' | '>=' | 'in' | 'not' 'in'
      | '|'
      | '^'
      | '&'
      | '<<' | '>>'
      | '-' | '+'
      | '*' | '%' | '/' | '//'
      .
*/

#[derive(Clone, Debug)]
pub struct BinaryExpression {
    pub left: Test,
    pub op: BinaryOp,
    pub right: Test,
}

/// The numeric value indicates the parsing priority.
#[derive(Clone, Copy, Debug)]
pub enum BinaryOp {
    Or = 1,
    And = 2,
    IsEqual = 3,
    NotEqual = 4,
    LessThan = 5,
    GreaterThan = 6,
    LessEqual = 7,
    GreaterEqual = 8,
    In = 9,
    NotIn = 10,
    BitOr = 11,
    Xor = 12,
    BitAnd = 13,
    ShiftLeft = 14,
    ShiftRight = 15,
    Subtract = 16,
    Add = 17,
    Multiply = 18,
    Modulus = 19,
    TrueDivide = 20,
    FloorDivide = 21,
}

/// Priority of matching the 'if' in the statement 'X if Y else Z'.
const IF_OP_PRIORITY: usize = 0;

const UNARY_OP_PRIORITY: usize = 100;

const DEFAULT_OP_PRIORITY: usize = 0;

/*
LambdaExpr = 'lambda' [Parameters] ':' Test .

*/

/// NOTE: An expression may have zero or more tests depending on who is creating
/// it.
#[derive(Clone, Debug)]
pub struct Expression {
    pub tests: Vec<Test>,
    pub has_trailing_comma: bool,
}

impl Expression {
    /// Parses a comma separated list of one or more Tests as an Expression.
    /// This does not parse any trailing comma.
    ///
    /// Grammar rule:
    ///   Expression = Test {',' Test} .
    ///   # NOTE: trailing comma permitted only when within [...] or (...).
    pub fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        let mut tests = vec![];
        let mut has_trailing_comma = false;

        tests.push(parse_next!(input, Test::parse, context));

        tests.extend(
            parse_next!(
                input,
                many(seq!(c => {
                    c.next(tag(","))?;
                    c.next(|v| context.consume_whitespace(v))?;

                    c.next(|v| Test::parse(v, context))
                }))
            )
            .into_iter(),
        );

        Ok((
            Self {
                tests,
                has_trailing_comma: false,
            },
            input,
        ))
    }
}

#[derive(Clone, Debug)]
pub enum Operand {
    Identifier(String),
    Int(i64),
    Float(f64),
    String(String),
    // TODO: Bytes(Vec<u8>),
    List(Expression),
    // TODO: ListComprehension
    Dict(Vec<(Test, Test)>),
    // TODO: Dictcomprehension
    Tuple(Expression),

    // These are defined in the syntax as they are most special than built in values (they can't
    // be overriden/shadowed)
    None,
    Bool(bool),
}

impl Operand {
    /*
    Operand = identifier
            | int | float | string | bytes
            | ListExpr | ListComp
            | DictExpr | DictComp
            | '(' [Expression [',']] ')'
            .

    ListExpr = '[' [Expression [',']] ']' .
    ListComp = '[' Test {CompClause} ']'.

    DictExpr = '{' [Entries [',']] '}' .
    DictComp = '{' Entry {CompClause} '}' .
    Entries  = Entry {',' Entry} .
    Entry    = Test ':' Test .

    CompClause = 'for' LoopVariables 'in' Test | 'if' Test .

    LoopVariables = PrimaryExpr {',' PrimaryExpr} .
    */
    fn parse<'a>(mut input: &'a str, context: &ParsingContext) -> Result<(Self, &'a str)> {
        let (value, rest) = alt!(
            map(identifier, |v| Self::Identifier(v)),
            map(float_lit, |v| Self::Float(v)),
            map(int_lit, |v| Self::Int(v as i64)),
            map(keyword("None"), |_| Self::None),
            map(keyword("True"), |_| Self::Bool(true)),
            map(keyword("False"), |_| Self::Bool(false)),
            seq!(c => {
                let vals = c.next(strLit)?;

                // TODO: In python '\ff\ff' is interpreted as 2 code points
                // TODO: But, '\u1020' is one code point
                // TODO: So strLit should emit a Vec<char>?

                let mut s = String::new();
                for b in vals {
                    s.push(b as char);
                }

                // TODO: Support triple quoted strings here and in the protobuf parser
                Ok(Self::String(s))
            }),
            // TODO: Add ListComp here
            seq!(c => {
                c.next(tag("["))?;

                let mut inner_context = context.clone();
                inner_context.in_parens = true;
                c.next(|v| inner_context.consume_whitespace(v))?;

                let expr = c.next(opt(|v| Expression::parse(v, &inner_context)))?;

                let expr = match expr {
                    Some(mut v) => {
                        v.has_trailing_comma = c.next(opt(tag(",")))?.is_some();
                        c.next(|v| inner_context.consume_whitespace(v))?;
                        v
                    }
                    None => {
                        Expression { tests: vec![], has_trailing_comma: false }
                    }
                };

                c.next(tag("]"))?;
                c.next(|v| context.consume_whitespace(v))?;
                Ok(Self::List(expr))
            }),
            // TODO: Add DictComp here
            seq!(c => {
                c.next(tag("{"))?;

                let mut inner_context = context.clone();
                inner_context.in_parens = true;
                c.next(|v| inner_context.consume_whitespace(v))?;

                let mut entries = vec![];

                loop {
                    let inner_context2 = inner_context.clone();
                    let entry = c.next(opt(seq!(c => {
                        let key = c.next(|v| Test::parse(v, &inner_context2))?;

                        c.next(tag(":"))?;
                        c.next(|v| inner_context2.consume_whitespace(v))?;

                        let value = c.next(|v| Test::parse(v, &inner_context2))?;

                        Ok((key, value))
                    })))?;

                    if let Some(entry) = entry {
                        entries.push(entry);
                    } else {
                        break;
                    }

                    if c.next(opt(tag(",")))?.is_some() {
                        c.next(|v| inner_context.consume_whitespace(v))?;
                    } else {
                        break;
                    }
                }

                c.next(tag("}"))?;
                c.next(|v| context.consume_whitespace(v))?;

                Ok(Self::Dict(entries))
            })
        )(input)?;

        let (_, rest2) = context.consume_whitespace(rest)?;
        Ok((value, rest2))
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn basic_parse() {
        println!("{:?}", File::parse("# hello").unwrap());
        println!("{:?}", File::parse("\"hello\"").unwrap());

        println!(
            "{:#?}",
            File::parse(
                "my_func_call(a = 2,
                b = \"Hello\", c = 124.5
            )"
            )
            .unwrap()
        );

        // TODO: 'not True if 0 else False' is '(not True) if 0 else False'

        // TODO: Test "a+=1"
        // TODO: Test "a=-1"
        // TODO: Test "a+b=c"
        // TODO: Test "a=b=2"
        // TODO: Test "1+-1"

        let test = r#"
def a(): # Inline comment
    return 2
        "#;

        // TODO: "a = 2; b = 3" is equal to "a = 2" \n "b = 3"
    }
}
