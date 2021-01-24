// Implementation of the protobuf plaintext format.
// aka what the C++ DebugString outputs and can re-parse as a proto.

use crate::reflection::{MessageReflection, ReflectionMut};
use crate::tokenizer::{float_lit, int_lit, strLit};
use common::errors::*;
use parsing::*;
use std::convert::TryInto;

/*

name :? {

}

name: "val\"ue"
name: 'val\'ue'
name: [{}, "", 1]

name: true|false
name: 123
name: 12.0
name: -4

name: "\\"
name: "\002" <- octal

name: ENUM_VALUE

[google.protobuf.Extension] :? {

}

input <
    hello: 123
>

TODO: According th this example, commas can be used after fields:
    input {
    dimension: [128, 8, 32, 32],
    data_type: DATA_HALF
    format: TENSOR_NCHW
  }

TODO: Text strings must appear on one line.

*/
//
const SYMBOLS: &'static str = "{}[]:,.";

enum TextToken {
    Whitespace,
    /// Starts with '#' and spans the rest of the current line.
    Comment,
    Symbol(char),
    /// Quotation mark delimited sequence of possibly encoded bytes.
    String(String),
    Identifier(String),
    // TODO: Support up to u64::MAX
    Integer(isize),
    Float(f64),
}

impl TextToken {
    parser!(parse<&str, TextToken> => alt!(
        map(Self::whitespace, |_| Self::Whitespace),
        map(Self::comment, |_| Self::Comment),
        map(Self::symbol, |v| Self::Symbol(v)),
        map(Self::string, |v| Self::String(v)),
        map(crate::tokenizer::ident, |v| Self::Identifier(v)),
        Self::number
    ));

    parser!(parse_filtered<&str, TextToken> => seq!(c => {
        c.next(many(and_then(Self::parse, |tok| {
            match tok {
                Self::Whitespace | Self::Comment => Ok(()),
                _ => Err(err_msg("Not whitespace/comment"))
            }
        })))?;

        c.next(Self::parse)
    }));

    parser!(whitespace<&str, &str> => slice(many1(one_of(" \t\r\n"))));
    parser!(comment<&str, &str> => seq!(c => {
        c.next(tag("#"))?;
        let end_marker = tag("\n");
        let inner = c.next(take_until(&end_marker))?;
        c.next(end_marker)?;
        Ok(inner)
    }));
    parser!(symbol<&str, char> => one_of(SYMBOLS));
    parser!(string<&str, String> => strLit);

    // TODO: Use the 'fullIdent' token type from the protobuf spec?
    // TODO: Should not allow two sequential dots?
    parser!(path<&str, &str> => {
        take_while1(
            |c: char| c.is_ascii_alphanumeric() ||c == '_' || c == '.' )
    });

    parser!(number<&str, Self> => seq!(c => {
        // TODO: Somewhat redundant with syntax.rs
        let sign = if c.next(opt(one_of("+-")))?.unwrap_or('+') == '+' { 1 } else { -1 };

        c.next(alt!(
            map(float_lit, |v| Self::Float((sign as f64) * v)),
            map(int_lit, |v| Self::Integer(sign * (v as isize)))
        ))
    }));
}

macro_rules! token_atom {
    ($name:ident, $e:ident, $t:ty) => {
        fn $name(input: &str) -> ParseResult<$t, &str> {
            match TextToken::parse_filtered(input)? {
                (TextToken::$e(s), rest) => Ok((s, rest)),
                _ => Err(err_msg("Wrong token")),
            }
        }
    };
}

token_atom!(symbol, Symbol, char);
token_atom!(string, String, String);
token_atom!(ident, Identifier, String);
token_atom!(integer, Integer, isize);
token_atom!(float, Float, f64);

// TODO: Dedup with syntax.rs
parser!(fullIdent<&str, String> => seq!(c => {
    let mut id = c.next(ident)?;

    while let Ok('.') = c.next(symbol) {
        id.push('.');

        let id_more = c.next(ident)?;
        id.push_str(id_more.as_str());
    }


    Ok(id)
}));

// TextMessage = TextField*
#[derive(Debug)]
struct TextMessage {
    fields: Vec<TextField>,
}

impl TextMessage {
    parser!(parse<&str, Self> => {
        // TODO: Each field may be optionally followed by a ','
        map(many(TextField::parse), |fields| Self { fields })
    });

    fn apply(&self, message: &mut dyn MessageReflection) -> Result<()> {
        for field in &self.fields {
            match &field.name {
                TextFieldName::Regular(name) => {
                    let field_num = message
                        .field_number_by_name(&name)
                        .ok_or_else(|| format_err!("Unknown field: {}", name))?;
                    let reflect = message.field_by_number_mut(field_num).unwrap();
                    field.value.apply(reflect)?;
                }
                TextFieldName::Extension(_) => {
                    return Err(err_msg("Extensions not supported"));
                }
            };
        }

        Ok(())
    }
}

// TextField = TextFieldName :?
#[derive(Debug)]
struct TextField {
    name: TextFieldName,
    value: TextValue,
}

impl TextField {
    parser!(parse<&str, Self> => seq!(c => {
        let name = c.next(TextFieldName::parse)?;
        println!("{:?}", name);
        // TODO: Also allowed to be a <
        let is_message = c.next(opt(peek(is(symbol, '{'))))?.is_some();
        if !is_message {
            c.next(is(symbol, ':'))?;
        }

        let value = c.next(TextValue::parse)?;
        Ok(Self { name, value })
    }));
}

#[derive(Debug)]
enum TextFieldName {
    Regular(String),
    Extension(String),
}

impl TextFieldName {
    parser!(parse<&str, Self> => alt!(
        seq!(c => {
            c.next(is(symbol, '['))?;
            let name = c.next(fullIdent)?;
            c.next(is(symbol, ']'))?;
            Ok(Self::Extension(name))
        }),
        map(ident, |s| Self::Regular(s))
    ));
}

#[derive(Debug)]
enum TextValue {
    Bool(bool),
    Integer(isize),
    Float(f64),

    // TODO: Verify if things like null bytes can appear in protobuf 'string'
    // types or just in 'bytes'
    /// String of bytes. Not necessarily UTF-8
    String(String),

    Identifier(String),

    Message(TextMessage),

    // TODO: Disallow array values inside of array values.
    Array(Vec<TextValue>),
}

impl TextValue {
    parser!(parse<&str, Self> => alt!(
        map(integer, |v| Self::Integer(v)),
        map(float, |v| Self::Float(v)),
        map(string, |v| Self::String(v)),
        // TODO: Add special cases for bools
        map(ident, |v| {
            if v == "true" {
                Self::Bool(true)
            } else if v == "false" {
                Self::Bool(false)
            } else {
                Self::Identifier(v)
            }
        }),
        seq!(c => {
            c.next(is(symbol, '{'))?;
            let val = c.next(TextMessage::parse)?;
            c.next(is(symbol, '}'))?;
            Ok(Self::Message(val))
        }),
        seq!(c => {
            c.next(is(symbol, '['))?;
            let values = c.next(delimited(Self::parse, is(symbol, ',')))?;
            c.next(is(symbol, ']'))?;
            Ok(Self::Array(values))
        })
    ));

    fn apply(&self, field: ReflectionMut) -> Result<()> {
        match field {
            // TODO: Whether we do down in precision, we should be cautious
            ReflectionMut::F32(v) => {
                *v = match self {
                    Self::Float(v) => *v as f32,
                    Self::Integer(v) => *v as f32,
                    _ => Err(err_msg("Can't cast to f32"))?,
                };
            }
            ReflectionMut::F64(v) => {
                *v = match self {
                    Self::Float(v) => *v,
                    Self::Integer(v) => *v as f64,
                    _ => {
                        return Err(err_msg("Can't cast to f64"));
                    }
                };
            }
            ReflectionMut::I32(v) => {
                *v = match self {
                    Self::Integer(v) => (*v).try_into()?,
                    _ => {
                        return Err(err_msg("Can't cast to i32"));
                    }
                };
            }
            ReflectionMut::I64(v) => {
                *v = match self {
                    Self::Integer(v) => (*v).try_into()?,
                    _ => {
                        return Err(err_msg("Can't cast to i64"));
                    }
                };
            }
            // TODO: If the text had a sign, then it should definately error out
            ReflectionMut::U32(v) => {
                *v = match self {
                    Self::Integer(v) => (*v).try_into()?,
                    _ => {
                        return Err(err_msg("Can't cast to u32"));
                    }
                };
            }
            ReflectionMut::U64(v) => {
                *v = match self {
                    Self::Integer(v) => (*v).try_into()?,
                    _ => {
                        return Err(err_msg("Can't cast to u64"));
                    }
                };
            }
            ReflectionMut::Bool(v) => {
                *v = match self {
                    Self::Bool(v) => *v,
                    _ => {
                        return Err(err_msg("Can't cast to bool"));
                    }
                };
            }
            ReflectionMut::Repeated(v) => {
                if let Self::Array(items) = self {
                    for item in items {
                        item.apply(v.add())?;
                    }
                } else {
                    self.apply(v.add())?;
                }
            }
            ReflectionMut::Message(v) => {
                if let Self::Message(m) = self {
                    m.apply(v)?;
                } else {
                    return Err(err_msg("Can't cast to message."));
                }
            }
            ReflectionMut::Enum(e) => match self {
                Self::Integer(v) => {
                    // TODO: Must verify that that we aren't losing precision.
                    e.assign(*v as i32)?;
                }
                _ => {}
            },
            _ => {
                return Err(err_msg("Unsupported reflect type"));
            }
        };

        Ok(())
    }
}

fn parse_text_syntax(text: &str) -> Result<TextMessage> {
    let (v, _) = complete(seq!(c => {
        let v = c.next(TextMessage::parse)?;
        // Can not end with any other meaningful tokens.
        c.next(many(alt!(
            map(TextToken::whitespace, |_| ()),
            map(TextToken::comment, |_| ())
        )))?;
        Ok(v)
    }))(text)?;

    Ok(v)
}

pub fn parse_text_proto(text: &str, message: &mut dyn MessageReflection) -> Result<()> {
    let v = parse_text_syntax(text)?;
    println!("{:#?}", v);

    v.apply(message)?;

    // TODO: Unless in a partial mode, we must now validate existence of required
    // fields.

    Ok(())
}

pub fn serialize_text_proto(message: &dyn MessageReflection) -> String {
    // Basically visiting a bunch of stuff.

    String::new()
}
