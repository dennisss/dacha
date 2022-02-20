// Implementation of the protobuf plaintext format.
// aka what the C++ DebugString outputs and can re-parse as a proto.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::convert::TryInto;
use core::ops::Deref;

use common::errors::*;
use parsing::*;

use crate::reflection::{MessageReflection, Reflection, ReflectionMut};
use crate::tokenizer::{float_lit, int_lit, serialize_str_lit, strLit};

//
const SYMBOLS: &'static str = "{}[]<>:,.";

enum TextToken {
    Whitespace,
    /// Starts with '#' and spans the rest of the current line.
    Comment,
    Symbol(char),
    /// Quotation mark delimited sequence of possibly encoded bytes.
    String(Vec<u8>),
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
    parser!(string<&str, Vec<u8>> => strLit);

    // TODO: Use the 'full_ident' token type from the protobuf spec?
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

// token(A, B, C) will create a function named 'A' which parses the next token
// in the input as a TextToken::B(C) (ignoring whitespace/comment tokens).
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
token_atom!(string, String, Vec<u8>);
token_atom!(ident, Identifier, String);
token_atom!(integer, Integer, isize);
token_atom!(float, Float, f64);

// TODO: Dedup with syntax.rs
parser!(full_ident<&str, String> => seq!(c => {
    let mut id = c.next(ident)?;

    while let Ok('.') = c.next(symbol) {
        id.push('.');

        let id_more = c.next(ident)?;
        id.push_str(id_more.as_str());
    }


    Ok(id)
}));

/// Represents the text format of a
// TextMessage = TextField*
#[derive(Debug)]
pub struct TextMessage {
    fields: Vec<TextField>,
}

impl TextMessage {
    parser!(parse<&str, Self> => {
        // TODO: Each field may be optionally followed by a ','
        map(delimited(TextField::parse, opt(is(symbol, ','))), |fields| Self { fields })
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
        let is_message = c.next(opt(peek(alt!(
            is(symbol, '{'),
            is(symbol, '<')
        ))))?.is_some();
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
            let name = c.next(full_ident)?;
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
    String(Vec<u8>),

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
            c.next(is(symbol, '<'))?;
            let val = c.next(TextMessage::parse)?;
            c.next(is(symbol, '>'))?;
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
            ReflectionMut::Set(v) => {
                // NOTE: Sets behave pretty much the same way as repeated fields.

                if let Self::Array(items) = self {
                    for item in items {
                        let mut e = v.entry_mut();
                        item.apply(e.value())?;
                        e.insert();
                    }
                } else {
                    let mut e = v.entry_mut();
                    self.apply(e.value())?;
                    e.insert();
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
                Self::Identifier(v) => {
                    e.assign_name(&v)?;
                }
                _ => {
                    return Err(err_msg("Can't cast to enum"));
                }
            },
            ReflectionMut::String(s) => match self {
                Self::String(s_value) => {
                    *s = std::str::from_utf8(&s_value[..])?.to_string();
                }
                _ => {
                    println!("Problematic: {:?}", self);
                    return Err(err_msg("Can't cast to string"));
                }
            },
            ReflectionMut::Bytes(bytes) => match self {
                Self::String(v) => {
                    bytes.clear();
                    bytes.extend_from_slice(&v[..]);
                }
                _ => {
                    return Err(err_msg("Can't cast to bytes"));
                }
            },
        };

        Ok(())
    }
}

/// Parses a text proto string into its raw components
///
/// NOTE: This function shoulnd't be used directly.
///
/// TODO: Make this a loseless parser so that we can perform formatting.
///
/// TODO: Long term this should be ideally a streaming interface for more
/// efficient parsing.
pub fn parse_text_syntax(text: &str) -> Result<TextMessage> {
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

    v.apply(message)?;

    // TODO: Unless in a partial mode, we must now validate existence of required
    // fields.

    Ok(())
}

pub trait ParseTextProto {
    fn parse_text(text: &str) -> Result<Self>
    where
        Self: Sized;
}

impl<T: Sized + Default + MessageReflection> ParseTextProto for T {
    fn parse_text(text: &str) -> Result<Self> {
        let mut m = T::default();
        parse_text_proto(text, &mut m)?;
        Ok(m)
    }
}

pub fn serialize_text_proto(message: &dyn MessageReflection) -> String {
    let mut out = String::new();
    serialize_message(message, "", &mut out);
    out
}

fn serialize_message(message: &dyn MessageReflection, indent: &str, out: &mut String) {
    for field in message.fields() {
        let refl = match message.field_by_number(field.number) {
            Some(v) => v,
            None => continue,
        };

        let is_message = match &refl {
            Reflection::Message(_) => true,
            _ => false,
        };

        let field_start_idx = out.len();

        out.push_str(&format!(
            "{}{}{} ",
            indent,
            field.name.deref(),
            if is_message { "" } else { ":" }
        ));

        let value_start_idx = out.len();

        serialize_reflection(refl, indent, out);

        // Empty value
        if out.len() == value_start_idx {
            out.truncate(field_start_idx);
            continue;
        }

        out.push_str("\n");
    }
}

fn serialize_reflection(refl: Reflection, indent: &str, out: &mut String) {
    match refl {
        // TODO: Check these float cases.
        // TODO: Ignore fields with default values?
        Reflection::F32(v) => {
            if *v == 0.0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::F64(v) => {
            if *v == 0.0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::I32(v) => {
            if *v == 0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::I64(v) => {
            if *v == 0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::U32(v) => {
            if *v == 0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::U64(v) => {
            if *v == 0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::Bool(v) => {
            if *v {
                out.push_str("true");
            } else {
                return;
                // out.push_str("false");
            }
        }
        Reflection::String(v) => {
            if v.is_empty() {
                return;
            }

            serialize_str_lit(v.as_bytes(), out);
        }
        Reflection::Bytes(v) => {
            if v.is_empty() {
                return;
            }
            serialize_str_lit(v, out);
        }
        Reflection::Repeated(v) => {
            if v.len() == 0 {
                return;
            }

            out.push_str("[\n");

            let inner_indent = format!("{}    ", indent);

            for i in 0..v.len() {
                out.push_str(&inner_indent);
                let r = v.get(i).unwrap();
                serialize_reflection(r, &inner_indent, out);

                if i != v.len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }

            out.push_str(&format!("{}]", indent));
        }
        Reflection::Set(s) => {
            if s.len() == 0 {
                return;
            }

            out.push_str("[\n");

            let inner_indent = format!("{}    ", indent);

            // TODO: Deduplicate with the repeated field code.
            let mut first = true;
            let mut iter = s.iter();
            while let Some(r) = iter.next() {
                if !first {
                    out.push_str(",\n");
                }
                first = false;

                out.push_str(&inner_indent);
                serialize_reflection(r, &inner_indent, out);
            }

            out.push_str(&format!("\n{}]", indent));
        }
        Reflection::Message(v) => {
            out.push_str("{\n");

            let initial_len = out.len();
            let inner_indent = format!("{}    ", indent);

            serialize_message(v, &inner_indent, out);

            if initial_len == out.len() {
                out.pop();
                out.push('}');
            } else {
                out.push_str(&format!("{}}}", indent));
            }
        }
        Reflection::Enum(v) => {
            if v.value() == 0 {
                return;
            }
            out.push_str(v.name());
        }
    }
}

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

#[cfg(test)]
mod test {
    use super::*;
    use crate::proto::test::*;

    #[test]
    fn parsing_test() {
        let mut list = ShoppingList::default();
        parse_text_proto(
            r#"
            # This is a comment
            name: "Groceries"
            id: 3
            cost: 12.50
            items: [
                # And here is another
                {
                    name: "First"
                    fruit_type: ORANGES
                },
                <
                    name: "Second",
                    fruit_type: APPLES
                >
            ]
            store: WALMART
            items {
                fruit_type: BERRIES
                name: 'Third'
            }
            "#,
            &mut list,
        )
        .unwrap();

        assert_eq!(list.name(), "Groceries");
        assert_eq!(list.id(), 3);
        assert_eq!(list.cost(), 12.5);
        assert_eq!(list.store(), ShoppingList_Store::WALMART);

        assert_eq!(list.items().len(), 3);

        println!("{:?}", list);
    }

    #[test]
    fn serialize_test() {
        let mut list = ShoppingList::default();
        assert_eq!(serialize_text_proto(&list), "");

        list.set_id(123);
        assert_eq!(serialize_text_proto(&list), "id: 123\n");

        list.set_name("Hi there!");
        assert_eq!(
            serialize_text_proto(&list),
            "name: \"Hi there!\"\nid: 123\n"
        );
    }
}
