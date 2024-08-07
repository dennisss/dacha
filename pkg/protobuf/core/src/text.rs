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

use crate::message_factory::MessageFactory;
use crate::reflection::{MessageReflection, Reflection, ReflectionMut};
use crate::tokenizer::{float_lit, int_lit, serialize_str_lit, strLit, whitespace};
use crate::FieldNumber;
use crate::Message;

//
const SYMBOLS: &'static str = "{}[]<>:,./";

enum TextToken {
    Whitespace,
    /// Starts with '#' and spans the rest of the current line.
    Comment,
    Symbol(char),
    /// Quotation mark delimited sequence of possibly encoded bytes.
    String(Vec<u8>),
    Identifier(String),
    // TODO: Support up to u64::MAX
    Integer(i64),
    Float(f64),
}

impl TextToken {
    parser!(parse<&str, TextToken> => alt!(
        map(whitespace, |_| Self::Whitespace),
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

    // parser!(whitespace<&str, &str> => slice(many1(one_of(" \t\r\n"))));

    parser!(comment<&str, &str> => seq!(c => {
        c.next(tag("#"))?;
        let end_marker = tag("\n");
        let inner = c.next(take_until(&end_marker))?;
        c.next(end_marker)?;
        Ok(inner)
    }));
    parser!(symbol<&str, char> => one_of(SYMBOLS));
    parser!(string<&str, Vec<u8>> => multiline_str_lit);

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
            map(int_lit, |v| Self::Integer(sign * (v as i64)))
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
token_atom!(integer, Integer, i64);
token_atom!(float, Float, f64);

parser!(pub multiline_str_lit<&str, Vec<u8>> => seq!(c => {
    let mut out = c.next(strLit)?;

    while let Some(extra_line) = c.next(opt(extra_strlit))? {
        out.extend_from_slice(&extra_line);
    }

    Ok(out)
}));

parser!(extra_strlit<&str, Vec<u8>> => seq!(c => {
    c.next(take_while(|c: char| c.is_whitespace() && c != '\n'))?;
    c.next(tag("\n"))?;
    c.next(take_while(|c: char| c.is_whitespace() && c != '\n'))?;
    c.next(strLit)
}));

// TODO: Dedup with syntax.rs
parser!(pub full_ident<&str, String> => seq!(c => {
    let mut id = c.next(ident)?;

    while let Ok(_) = c.next(is(symbol, '.')) {
        id.push('.');

        let id_more = c.next(ident)?;
        id.push_str(id_more.as_str());
    }


    Ok(id)
}));

parser!(extension_name<&str, String> => seq!(c => {
    let mut id = c.next(full_ident)?;

    while let Ok(_) = c.next(is(symbol, '/')) {
        id.push('/');

        let id_more = c.next(full_ident)?;
        id.push_str(id_more.as_str());
    }

    Ok(id)
}));

pub struct TextMessageFile {
    proto_file: Option<String>,
    proto_message: Option<String>,

    message: TextMessage,
}

impl TextMessageFile {
    pub fn parse(mut input: &str) -> Result<Self> {
        let mut proto_file = None;
        let mut proto_message = None;

        while let Some(comment) = parse_next!(input, opt(TextToken::comment)) {
            if let Some(file) = comment.strip_prefix(" proto-file: ") {
                proto_file = Some(file.to_string());
            } else if let Some(message) = comment.strip_prefix(" proto-message: ") {
                proto_message = Some(message.to_string());
            }
        }

        let message = parse_text_syntax(input)?;

        Ok(Self {
            proto_file,
            proto_message,
            message,
        })
    }

    pub fn proto_file(&self) -> Option<&str> {
        self.proto_file.as_ref().map(|v| v.as_ref())
    }

    pub fn proto_message(&self) -> Option<&str> {
        self.proto_message.as_ref().map(|v| v.as_ref())
    }

    pub fn merge_to(
        &self,
        message: &mut dyn MessageReflection,
        options: &ParseTextProtoOptions,
    ) -> Result<()> {
        self.message.apply(message, options)
    }
}

/// Represents the text format of a
// TextMessage = TextField*
#[derive(Debug, Clone, PartialEq)]
pub struct TextMessage {
    fields: Vec<TextField>,
}

impl TextMessage {
    parser!(pub parse_value<&str, Self> => {
        alt!(
            seq!(c => {
                c.next(is(symbol, '{'))?;
                let val = c.next(Self::parse)?;
                c.next(is(symbol, '}'))?;
                Ok(val)
            }),
            seq!(c => {
                c.next(is(symbol, '<'))?;
                let val = c.next(Self::parse)?;
                c.next(is(symbol, '>'))?;
                Ok(val)
            })
        )
    });

    parser!(parse<&str, Self> => {
        // TODO: Each field may be optionally followed by a ','
        map(delimited(TextField::parse, opt(is(symbol, ','))), |fields| Self { fields })
    });

    pub fn to_string(&self) -> String {
        let mut out = String::new();

        for field in &self.fields {
            out.push_str(&field.to_string());
            out.push(' ');
        }

        out
    }

    fn apply(
        &self,
        message: &mut dyn MessageReflection,
        options: &ParseTextProtoOptions,
    ) -> Result<()> {
        // TODO: Figure out if there is a better way to make this directly depend on the
        // type iur
        const ANY_TYPE_URL_FIELD_NUM: FieldNumber = 1; // Any::TYPE_URL_FIELD_NUM
        const ANY_VALUE_FIELD_NUM: FieldNumber = 2; // Any::VALUE_FIELD_NUM

        for field in &self.fields {
            match &field.name {
                TextFieldName::Regular(name) => {
                    let field_num = message
                        .field_number_by_name(&name)
                        .ok_or_else(|| format_err!("Unknown field: {}", name))?;
                    let reflect = message.field_by_number_mut(field_num).unwrap();
                    field.value.apply(reflect, options)?;
                }
                TextFieldName::Extension(path) => {
                    if message.type_url() != "type.googleapis.com/google.protobuf.Any" {
                        return Err(err_msg("Only Any proto text extensions are supported."));
                    }

                    let message_factory = options.message_factory.clone().ok_or_else(|| {
                        err_msg("Can not parse Any textprotos without a message factory")
                    })?;

                    let mut inner_message = message_factory
                        .new_message(path.as_str())
                        .ok_or_else(|| format_err!("Unknown message type: {}", path))?;
                    field
                        .value
                        .apply(ReflectionMut::Message(inner_message.as_mut()), options)?;

                    if let Some(ReflectionMut::String(v)) =
                        message.field_by_number_mut(ANY_TYPE_URL_FIELD_NUM)
                    {
                        *v = inner_message.type_url().to_string();
                    } else {
                        return Err(err_msg("Failed to find type_url field of Any proto"));
                    }

                    if let Some(ReflectionMut::Bytes(v)) =
                        message.field_by_number_mut(ANY_VALUE_FIELD_NUM)
                    {
                        v.clear();
                        v.extend_from_slice(&inner_message.serialize()?);
                    }
                }
            };
        }

        Ok(())
    }
}

// TextField = TextFieldName :?
#[derive(Debug, Clone, PartialEq)]
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

    pub fn to_string(&self) -> String {
        format!("{}: {}", self.name.to_string(), self.value.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TextFieldName {
    Regular(String),
    Extension(String),
}

impl TextFieldName {
    parser!(parse<&str, Self> => alt!(
        seq!(c => {
            c.next(is(symbol, '['))?;
            let name = c.next(extension_name)?;
            c.next(is(symbol, ']'))?;
            Ok(Self::Extension(name))
        }),
        map(ident, |s| Self::Regular(s))
    ));

    pub fn to_string(&self) -> String {
        match self {
            TextFieldName::Regular(v) => v.to_string(),
            TextFieldName::Extension(v) => format!("[{}]", v),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TextValue {
    Bool(bool),
    Integer(i64),
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
        map(TextMessage::parse_value, |v| Self::Message(v)),
        seq!(c => {
            c.next(is(symbol, '['))?;
            let values = c.next(delimited(Self::parse, is(symbol, ',')))?;
            c.next(is(symbol, ']'))?;
            Ok(Self::Array(values))
        })
    ));

    pub fn to_string(&self) -> String {
        match self {
            TextValue::Bool(v) => v.to_string(),
            TextValue::Integer(v) => v.to_string(),
            TextValue::Float(v) => v.to_string(),
            TextValue::String(v) => {
                let mut out = String::new();
                serialize_str_lit(&v[..], &mut out);
                out
            }
            TextValue::Identifier(v) => v.clone(),
            TextValue::Message(v) => format!("{{ {} }}", v.to_string()),
            TextValue::Array(v) => {
                let mut out = String::new();
                out.push('[');

                for i in 0..v.len() {
                    if i != 0 {
                        out.push(',');
                    }

                    out.push_str(&v[i].to_string());
                }

                out.push(']');
                out
            }
        }
    }

    fn apply(&self, field: ReflectionMut, options: &ParseTextProtoOptions) -> Result<()> {
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
                        item.apply(v.reflect_add(), options)?;
                    }
                } else {
                    self.apply(v.reflect_add(), options)?;
                }
            }
            ReflectionMut::Set(v) => {
                // NOTE: Sets behave pretty much the same way as repeated fields.

                if let Self::Array(items) = self {
                    for item in items {
                        let mut e = v.entry_mut();
                        item.apply(e.value(), options)?;
                        e.insert();
                    }
                } else {
                    let mut e = v.entry_mut();
                    self.apply(e.value(), options)?;
                    e.insert();
                }
            }
            ReflectionMut::Message(v) => {
                if let Self::Message(m) = self {
                    m.apply(v, options)?;
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

#[derive(Default)]
pub struct ParseTextProtoOptions<'a> {
    /// Message factory used to instantiate protos inside of Any protos.
    pub message_factory: Option<&'a dyn MessageFactory>,
}

/// Parses a text proto string into its raw components
///
/// NOTE: This function shoulnd't be used directly.
///
/// TODO: Make this a loseless parser so that we can perform formatting.
///
/// TODO: Long term this should be ideally a streaming interface for more
/// efficient parsing.
fn parse_text_syntax(text: &str) -> Result<TextMessage> {
    let (v, _) = complete(seq!(c => {
        let v = c.next(TextMessage::parse)?;
        // Can not end with any other meaningful tokens.
        c.next(many(alt!(
            map(whitespace, |_| ()),
            map(TextToken::comment, |_| ())
        )))?;
        Ok(v)
    }))(text)?;

    Ok(v)
}

pub fn parse_text_proto_with_options(
    text: &str,
    message: &mut dyn MessageReflection,
    options: &ParseTextProtoOptions,
) -> Result<()> {
    let v = parse_text_syntax(text)?;

    v.apply(message, options)?;

    // TODO: Unless in a partial mode, we must now validate existence of required
    // fields.

    Ok(())
}

pub fn parse_text_proto(text: &str, message: &mut dyn MessageReflection) -> Result<()> {
    parse_text_proto_with_options(text, message, &ParseTextProtoOptions::default())
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
        if !message.has_field_with_number(field.number) {
            continue;
        }

        let refl = message.field_by_number(field.number).unwrap();

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

        serialize_reflection(refl, indent, out, true);

        // Empty value
        if out.len() == value_start_idx {
            out.truncate(field_start_idx);
            continue;
        }

        out.push_str("\n");
    }
}

fn serialize_reflection(refl: Reflection, indent: &str, out: &mut String, sparse: bool) {
    match refl {
        // TODO: Check these float cases.
        // TODO: Ignore fields with default values?
        Reflection::F32(v) => {
            if sparse && *v == 0.0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::F64(v) => {
            if sparse && *v == 0.0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::I32(v) => {
            if sparse && *v == 0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::I64(v) => {
            if sparse && *v == 0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::U32(v) => {
            if sparse && *v == 0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::U64(v) => {
            if sparse && *v == 0 {
                return;
            }
            out.push_str(&v.to_string());
        }
        Reflection::Bool(v) => {
            if sparse && !v {
                return;
            }

            if *v {
                out.push_str("true");
            } else {
                out.push_str("false");
            }
        }
        Reflection::String(v) => {
            if sparse && v.is_empty() {
                return;
            }

            serialize_str_lit(v.as_bytes(), out);
        }
        Reflection::Bytes(v) => {
            if sparse && v.is_empty() {
                return;
            }
            serialize_str_lit(v, out);
        }
        Reflection::Repeated(v) => {
            if sparse && v.reflect_len() == 0 {
                return;
            }

            out.push_str("[\n");

            let inner_indent = format!("{}    ", indent);

            for i in 0..v.reflect_len() {
                out.push_str(&inner_indent);
                let r = v.reflect_get(i).unwrap();
                serialize_reflection(r, &inner_indent, out, false);

                if i != v.reflect_len() - 1 {
                    out.push(',');
                }
                out.push('\n');
            }

            out.push_str(&format!("{}]", indent));
        }
        Reflection::Set(s) => {
            if sparse && s.len() == 0 {
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
                serialize_reflection(r, &inner_indent, out, false);
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

    // TODO: Handle extensions and unknown fields
}

/*
Given a MessageReflection and a

Eventually extensions would look something like:

```
local_field: 10
[com.example.ext_field]​: 20
```

*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_extensions() {
        let data = r#"
        platform {
            os: LINUX
            architecture: AMD64
        }
        
        rule_defaults {
            key: "rust_binary"
            value {
                [type.googleapis.com/builder.RustBinary] {
                    compiler: CARGO
                    profile: "release"
                    target: "x86_64-unknown-linux-gnu"
                }
            }
        }        
        "#;

        parse_text_syntax(data).unwrap();
    }

    #[test]
    fn multi_line_string() {
        let data = r#"
        text:
            "hello"
            " world"
            "!" 
        "#;

        assert_eq!(
            parse_text_syntax(data).unwrap(),
            TextMessage {
                fields: vec![TextField {
                    name: TextFieldName::Regular("text".to_string()),
                    value: TextValue::String(b"hello world!".to_vec())
                }]
            }
        );
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
