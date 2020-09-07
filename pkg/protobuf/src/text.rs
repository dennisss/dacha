// Implementation of the protobuf plaintext format.
// aka what the C++ DebugString outputs and can re-parse as a proto.

use crate::tokenizer::{floatLit, intLit, strLit};
use common::errors::*;
use parsing::*;

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
            map(floatLit, |v| Self::Float((sign as f64) * v)),
            map(intLit, |v| Self::Integer(sign * (v as isize)))
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
        map(ident, |v| Self::Identifier(v)),
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

pub fn parse_text_proto(text: &str) -> Result<()> {
    let v = parse_text_syntax(text)?;
    println!("{:#?}", v);

    Ok(())
}
