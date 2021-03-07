// Syntax of the .proto files for version 2
// Based on https://developers.google.com/protocol-buffers/docs/reference/proto2-spec
//
// https://developers.google.com/protocol-buffers/docs/reference/proto3-spec

// |   alternation
// ()  grouping
// []  option (zero or one time)
// {}  repetition (any number of times)

use crate::spec::*;
use crate::tokenizer::{capitalLetter, decimal_digit, letter, Token};
use common::errors::*;
use parsing::*;

macro_rules! token_atom {
    ($name:ident, $e:ident, $t:ty) => {
        fn $name(input: &str) -> ParseResult<$t, &str> {
            match Token::parse_filtered(input)? {
                (Token::$e(s), rest) => Ok((s, rest)),
                _ => Err(err_msg("Wrong token")),
            }
        }
    };
}

// Wrappers for reading a single type of token and returning the inner
// representation
token_atom!(ident, Identifier, String);
token_atom!(floatLit, Float, f64);
token_atom!(intLit, Integer, usize);
token_atom!(symbol, Symbol, char);
token_atom!(strLit, String, String);

// Proto 2 and 3
// fullIdent = ident { "." ident }
parser!(fullIdent<&str, String> => seq!(c => {
    let mut id = c.next(ident)?;

    while let Ok('.') = c.next(symbol) {
        id.push('.');

        let id_more = c.next(ident)?;
        id.push_str(id_more.as_str());
    }


    Ok(id)
}));

// Proto 2 and 3
parser!(enumName<&str, String> => ident);
parser!(messageName<&str, String> => ident);
parser!(fieldName<&str, String> => ident);
parser!(oneofName<&str, String> => ident);
parser!(mapName<&str, String> => ident);
parser!(serviceName<&str, String> => ident);
parser!(rpcName<&str, String> => ident);
parser!(streamName<&str, String> => ident);

// Proto 2 and 3
// messageType = [ "." ] { ident "." } messageName
parser!(messageType<&str, String> => seq!(c => {
    let mut s = String::new();
    if let Ok(dot) = c.next(is(symbol, '.')) {
        s.push(dot);
    }

    let path = c.next(many(seq!(c => {
        let mut id = c.next(ident)?;
        id.push(c.next(is(symbol, '.'))?);
        Ok(id)
    })))?;

    s.push_str(&path.join(""));

    let name = c.next(messageName)?;
    s.push_str(name.as_str());

    Ok(s)
}));

// Proto 2 and 3
// enumType = [ "." ] { ident "." } enumName
parser!(enumType<&str, String> => {
    // TODO: Instead internally use enumName instead of messageName
    messageType
});

// Proto 2
// groupName = capitalLetter { letter | decimalDigit | "_" }
parser!(groupName<&str, String> => seq!(c => {
    let id = c.next(ident)?;

    for (i, c) in id.chars().enumerate() {
        let valid = if i == 0 {
            capitalLetter(c)
        } else {
            letter(c) || decimal_digit(c) || c == '_'
        };

        if !valid {
            return Err(err_msg("Invalid group name"));
        }
    }

    Ok(id)
}));

// Proto 2 and 3
// boolLit = "true" | "false"
parser!(boolLit<&str, bool> => seq!(c => {
    let id = c.next(ident)?;
    let val = match id.as_ref() {
        "true" => true,
        "false" => false,
        _ => return Err(err_msg("Expected true|false"))
    };

    Ok(val)
}));

// Proto 2 and 3
// emptyStatement = ";"
parser!(emptyStatement<&str, char> => is(symbol, ';'));

fn sign(input: &str) -> ParseResult<isize, &str> {
    let (c, rest) = symbol(input)?;
    match c {
        '+' => Ok((1, rest)),
        '-' => Ok((-1, rest)),
        _ => Err(err_msg("Invalid sign")),
    }
}

// TODO: Can be combined with floatValue
parser!(intValue<&str, isize> => seq!(c => {
    let sign: isize = c.next(sign).unwrap_or(1);
    let f = c.next(intLit)?;
    Ok(sign * (f as isize))
}));

parser!(floatValue<&str, f64> => seq!(c => {
    let sign: isize = c.next(sign).unwrap_or(1);
    let f = c.next(floatLit)?;
    Ok((sign as f64) * f)
}));

// TODO: Update this
// Proto 2 and 3
// constant = fullIdent | ( [ "-" | "+" ] intLit ) | ( [ "-" | "+" ] floatLit )
// |                 strLit | boolLit
parser!(constant<&str, Constant> => seq!(c => {
    let str_const = |input| {
        strLit(input).map(|(s, rest)| (Constant::String(s), rest))
    };

    let bool_const = |input| {
        boolLit(input).map(|(b, rest)| (Constant::Bool(b), rest))
    };

    c.next(alt!(
        map(fullIdent, |s| Constant::Identifier(s)),
        map(intValue, |i| Constant::Integer(i)),
        map(floatValue, |f| Constant::Float(f)),
        str_const,
        bool_const
    ))
}));

// syntax = "syntax" "=" quote "proto2" quote ";"
parser!(pub syntax<&str, Syntax> => seq!(c => {
    c.next(is(ident, "syntax"))?;
    c.next(is(symbol, '='))?;
    let s = c.next(is(strLit, "proto2")).map(|_| Syntax::Proto2)
        .or_else(|_| c.next(is(strLit, "proto3")).map(|_| Syntax::Proto3))?;
    c.next(is(symbol, ';'))?;
    Ok(s)
}));

// Proto 2 and 3
// import = "import" [ "weak" | "public" ] strLit ";"
parser!(import<&str, Import> => seq!(c => {
    c.next(is(ident, "import"))?;

    let mut typ = c.next(is(ident, "weak")).map(|_| ImportType::Weak)
        .or_else(|_| c.next(is(ident, "public")).map(|_| ImportType::Public))
        .unwrap_or(ImportType::Default);
    let path = c.next(strLit)?;
    c.next(is(symbol, ';'))?;
    Ok(Import { typ, path })
}));

// Proto 2 and 3
// package = "package" fullIdent ";"
parser!(package<&str, String> => seq!(c => {
    c.next(is(ident, "package"))?;
    let name = c.next(fullIdent)?;
    c.next(is(symbol, ';'))?;
    Ok(name)
}));

// Proto 2 and 3
// option = "option" optionName  "=" constant ";"
parser!(option<&str, Opt> => seq!(c => {
    c.next(is(ident, "option"))?;
    let name = c.next(optionName)?;
    let value = c.next(constant)?;
    c.next(is(symbol, ';'))?;
    Ok(Opt { name, value })
}));

// Proto 2 and 3
// optionName = ( ident | "(" fullIdent ")" ) { "." ident }
parser!(optionName<&str, String> => seq!(c => {
    let prefix = c.next(ident)
        .or_else(|_| c.next(seq!(c => {
            c.next(is(symbol, '('))?;
            let s = c.next(fullIdent)?;
            c.next(is(symbol, ')'))?;
            Ok(String::from("(") + &s + &")")
        })))?;

    let rest = c.many(seq!(c => {
        c.next(is(symbol, '.'))?;
        let id = c.next(ident)?;
        Ok(String::from(".") + &id)
    }));

    Ok(prefix + &rest.join(""))
}));

// Proto 2: Required | Optional | Repeated
// Proto 3: None | Repeated
//
// label = ("required" | "optional" | "repeated") ?
parser!(label<&str, Label> => seq!(c => {
    let label = c.next(is(ident, "required")).map(|_| Label::Required)
        .or_else(|_| c.next(is(ident, "optional")).map(|_| Label::Optional))
        .or_else(|_| c.next(is(ident, "repeated")).map(|_| Label::Repeated))
        .unwrap_or(Label::None);
    Ok(label)
}));

// Proto 2 and 3
// type = "double" | "float" | "int32" | "int64" | "uint32" | "uint64"
//       | "sint32" | "sint64" | "fixed32" | "fixed64" | "sfixed32" | "sfixed64"
//       | "bool" | "string" | "bytes" | messageType | enumType
parser!(fieldType<&str, FieldType> => seq!(c => {
    let primitive = seq!(c => {
        let name = c.next(ident)?;
        let t = match name.as_str() {
            "double" => FieldType::Double,
            "float" => FieldType::Float,
            "int32" => FieldType::Int32,
            "int64" => FieldType::Int64,
            "uint32" => FieldType::Uint32,
            "uint64" => FieldType::Uint64,
            "sint32" => FieldType::Sint32,
            "sint64" => FieldType::Sint64,
            "fixed32" => FieldType::Fixed32,
            "fixed64" => FieldType::Sfixed64,
            "sfixed32" => FieldType::Sfixed32,
            "sfixed64" => FieldType::Sfixed64,
            "bool" => FieldType::Bool,
            "string" => FieldType::String,
            "bytes" => FieldType::Bytes,
            _ => { return Err(err_msg("Unknown data type")); }
        };

        Ok(t)
    });

    let t = c.next(primitive)
        .or_else(|_| c.next(messageType).map(|n| FieldType::Named(n)))?;

    Ok(t)
}));

// Proto 2 and 3
// fieldNumber = intLit;
parser!(fieldNumber<&str, FieldNumber> => map(intLit, |v| v as FieldNumber));

// TODO: In proto 3, 'label' should be replaced with '[ "repeated" ]'
// field = label type fieldName "=" fieldNumber [ "[" fieldOptions "]" ] ";"
parser!(field<&str, Field> => seq!(c => {
    let labl = c.next(label)?;
    let typ = c.next(fieldType)?;
    let name = c.next(fieldName)?;
    c.next(is(symbol, '='))?;
    let num = c.next(fieldNumber)?;
    let unknown_options = c.next(fieldOptionsWrap).unwrap_or(vec![]);

    c.next(is(symbol, ';'))?;

    Ok(Field {
        label: labl, typ, name, num, options: FieldOptions::default(),
        unknown_options
    })
}));

// Proto 2 and 3
// Not on the official grammar page, but useful to reuse.
// "[" fieldOptions "]"
parser!(fieldOptionsWrap<&str, Vec<Opt>> => seq!(c => {
    c.next(is(symbol, '['))?;
    let list = c.next(fieldOptions)?;
    c.next(is(symbol, ']'))?;
    Ok(list)
}));

//

parser!(comma<&str, char> => is(symbol, ','));

// Proto 2 and 3
// fieldOptions = fieldOption { ","  fieldOption }
parser!(fieldOptions<&str, Vec<Opt>> => delimited1(fieldOption, comma));

// Proto 2 and 3
// fieldOption = optionName "=" constant
parser!(fieldOption<&str, Opt> => seq!(c => {
    let name = c.next(optionName)?;
    c.next(is(symbol, '='))?;
    let value = c.next(constant)?;
    Ok(Opt { name, value })
}));

// Proto 2
// group = label "group" groupName "=" fieldNumber messageBody
parser!(group<&str, Group> => seq!(c => {
    let lbl = c.next(label)?;
    c.next(is(ident, "group"))?;
    let name = c.next(groupName)?;
    c.next(is(symbol, '='))?;
    let num = c.next(fieldNumber)?;
    let body = c.next(messageBody)?;
    Ok(Group { label: lbl, name, num, body })
}));

// Proto 2 and 3
// oneof = "oneof" oneofName "{" { oneofField | emptyStatement } "}"
parser!(oneof<&str, OneOf> => seq!(c => {
    c.next(is(ident, "oneof"))?;
    let name = c.next(oneofName)?;
    c.next(is(symbol, '{'))?;
    let fields = c.many(seq!(c => {
        let f = c.next(oneofField).map(|f| Some(f))
            .or_else(|_| c.next(emptyStatement).map(|_| None))?;
        Ok(f)
    })).into_iter().filter_map(|x| x).collect::<Vec<_>>();
    c.next(is(symbol, '}'))?;
    Ok(OneOf { name, fields })
}));

// Proto 2 and 3
// oneofField = type fieldName "=" fieldNumber [ "[" fieldOptions "]" ] ";"
parser!(oneofField<&str, Field> => seq!(c => {
    let typ = c.next(fieldType)?;
    let name = c.next(fieldName)?;
    c.next(is(symbol, '='))?;
    let num = c.next(fieldNumber)?;
    let unknown_options = c.next(fieldOptionsWrap).unwrap_or(vec![]);
    c.next(is(symbol, ';'))?;
    Ok(Field { label: Label::Optional, typ, name,
        num, options: FieldOptions::default(), unknown_options })
}));

// Proto 2 and 3
// mapField = "map" "<" keyType "," type ">" mapName "=" fieldNumber [ "["
// fieldOptions "]" ] ";"
parser!(mapField<&str, MapField> => seq!(c => {
    c.next(is(ident, "map"))?;
    c.next(is(symbol, '<'))?;
    let key_type = c.next(keyType)?;
    c.next(is(symbol, ','))?;
    let value_type = c.next(fieldType)?;
    c.next(is(symbol, '>'))?;
    let name = c.next(mapName)?;
    c.next(is(symbol, '='))?;
    let num = c.next(fieldNumber)?;
    let options = c.next(fieldOptionsWrap).unwrap_or(vec![]);
    c.next(is(symbol, ';'))?;
    Ok(MapField { key_type, value_type, name, num, options })
}));

// Proto 2 and 3
// keyType = "int32" | "int64" | "uint32" | "uint64" | "sint32" | "sint64" |
//           "fixed32" | "fixed64" | "sfixed32" | "sfixed64" | "bool" | "string"
parser!(keyType<&str, FieldType> => seq!(c => {
    let name = c.next(ident)?;
    let t = match name.as_str() {
        "int32" => FieldType::Int32,
        "int64" => FieldType::Int64,
        "uint32" => FieldType::Uint32,
        "uint64" => FieldType::Uint64,
        "sint32" => FieldType::Sint32,
        "sint64" => FieldType::Sint64,
        "fixed32" => FieldType::Fixed32,
        "fixed64" => FieldType::Fixed64,
        "sfixed32" => FieldType::Sfixed32,
        "sfixed64" => FieldType::Sfixed64,
        "bool" => FieldType::Bool,
        "string" => FieldType::String,
        _ => { return Err(err_msg("Invalid key type")); }
    };

    Ok(t)
}));

// Proto 2
// extensions = "extensions" ranges ";"
parser!(extensions<&str, Ranges> => seq!(c => {
    c.next(is(ident, "extensions"))?;
    let out = c.next(ranges)?;
    c.next(is(symbol, ';'))?;
    Ok(out)
}));

// Proto 2 and 3
// ranges = range { "," range }
parser!(ranges<&str, Ranges> => delimited1(range, comma));

// Proto 2 and 3
// range =  intLit [ "to" ( intLit | "max" ) ]
parser!(range<&str, Range> => seq!(c => {
    let lower = c.next(intLit)?;

    let upper_parser = seq!(c => {
        c.next(is(ident, "to"))?;
        let v = c.next(intLit)
            .or_else(|_| c.next(is(ident, "max")).map(|_| std::usize::MAX))?;
        Ok(v)
    });

    let upper = c.next(upper_parser)?;
    Ok((lower as u32, upper  as u32))
}));

// Proto 2 and 3
// reserved = "reserved" ( ranges | fieldNames ) ";"
parser!(reserved<&str, Reserved> => seq!(c => {
    c.next(is(ident, "reserved"))?;
    let val = c.next(ranges).map(|rs| Reserved::Ranges(rs))
        .or_else(|_| c.next(fieldNames).map(|ns| Reserved::Fields(ns)))?;
    c.next(is(symbol, ';'))?;
    Ok(val)
}));

// Proto 2 and 3
// fieldNames = fieldName { "," fieldName }
parser!(fieldNames<&str, Vec<String>> => delimited1(fieldName, comma));

// Proto 2 and 3
// enum = "enum" enumName enumBody
parser!(enum_<&str, Enum> => seq!(c => {
    c.next(is(ident, "enum"))?;
    let name = c.next(enumName)?;
    let body = c.next(enumBody)?;
    Ok(Enum { name, body })
}));

// Proto 2 and 3
// enumBody = "{" { option | enumField | emptyStatement } "}"
parser!(enumBody<&str, Vec<EnumBodyItem>> => seq!(c => {
    c.next(is(symbol, '{'))?;
    let inner = c.many(seq!(c => {
        let item = c.next(option).map(|o| Some(EnumBodyItem::Option(o)))
            .or_else(|_| c.next(enumField).map(|f| Some(EnumBodyItem::Field(f))))
            .or_else(|_| c.next(emptyStatement).map(|_| None))?;
        Ok(item)
    })).into_iter().filter_map(|x| x).collect::<Vec<_>>();
    c.next(is(symbol, '}'))?;
    Ok(inner)
}));

// Proto 2 and 3
// enumField = ident "=" [ "-" ] intLit [ "[" enumValueOption { ","
//             enumValueOption } "]" ]";"
parser!(enumField<&str, EnumField> => seq!(c => {
    let name = c.next(ident)?;
    c.next(is(symbol, '='))?;
    let is_negative = c.next(opt(is(symbol, '-')))?.is_some();
    let num = (c.next(intLit)? as i32) * if is_negative { -1 } else { 1 };
    let options = c.next(seq!(c => {
        c.next(is(symbol, '['))?;
        let opts = c.next(delimited1(enumValueOption, comma))?;
        c.next(is(symbol, ']'))?;
        Ok(opts)
    })).unwrap_or(vec![]);
    c.next(is(symbol, ';'))?;

    Ok(EnumField { name, num, options })
}));

// Proto 2 and 3
// enumValueOption = optionName "=" constant
parser!(enumValueOption<&str, Opt> => {
    fieldOption
});

// Proto 2 and 3
// message = "message" messageName messageBody
parser!(message<&str, Message> => seq!(c => {
    c.next(is(ident, "message"))?;
    let name = c.next(messageName)?;
    let body = c.next(messageBody)?;
    Ok(Message { name, body })
}));

// TODO: Proto3 has no 'extensions' or 'group'
// messageBody = "{" { field | enum | message | extend | extensions | group |
// option | oneof | mapField | reserved | emptyStatement } "}"
parser!(messageBody<&str, Vec<MessageItem>> => seq!(c => {
    c.next(is(symbol, '{'))?;

    let items = c.many(alt!(
        map(field, |v| Some(MessageItem::Field(v))),
        map(enum_, |v| Some(MessageItem::Enum(v))),
        map(message, |v| Some(MessageItem::Message(v))),
        map(extend, |v| Some(MessageItem::Extend(v))),
        map(extensions, |v| Some(MessageItem::Extensions(v))),
        map(oneof, |v| Some(MessageItem::OneOf(v))),
        map(mapField, |v| Some(MessageItem::MapField(v))),
        map(reserved, |v| Some(MessageItem::Reserved(v))),
        map(emptyStatement, |v| None)
    )).into_iter().filter_map(|x| x).collect::<Vec<_>>();

    c.next(is(symbol, '}'))?;
    Ok(items)
}));

// Proto 2
// extend = "extend" messageType "{" {field | group | emptyStatement} "}"
parser!(extend<&str, Extend> => seq!(c => {
    c.next(is(ident, "extend"))?;
    let typ = c.next(messageType)?;
    c.next(is(symbol, '{'))?;
    let body = c.many(seq!(c => {
        let item = c.next(field).map(|f| Some(ExtendItem::Field(f)))
            .or_else(|_| c.next(group).map(|g| Some(ExtendItem::Group(g))))
            .or_else(|_| c.next(emptyStatement).map(|_| None))?;
        Ok(item)
    })).into_iter().filter_map(|x| x).collect::<Vec<_>>();
    c.next(is(symbol, '}'))?;
    Ok(Extend { typ, body })
}));

// TODO: Proto 3 has no 'stream'
// service = "service" serviceName "{" { option | rpc | stream | emptyStatement
// } "}"
parser!(service<&str, Service> => seq!(c => {
    c.next(is(ident, "service"))?;
    let name = c.next(serviceName)?;
    c.next(is(symbol, '{'))?;
    let body = c.many(alt!(
        map(option, |v| Some(ServiceItem::Option(v))),
        map(rpc, |v| Some(ServiceItem::RPC(v))),
        map(stream, |v| Some(ServiceItem::Stream(v))),
        map(emptyStatement, |_| None)
    )).into_iter().filter_map(|x| x).collect::<Vec<_>>();
    c.next(is(symbol, '}'))?;
    Ok(Service { name, body })
}));

// ( "{" { option | emptyStatement } "}" ) | ";"
parser!(options_body<&str, Vec<Opt>> => seq!(c => {
    let options_parser = seq!(c => {
        c.next(is(symbol, '{'))?;
        let opts = c.many(seq!(c => {
            let item = c.next(option).map(|o| Some(o))
                .or_else(|_| c.next(emptyStatement).map(|_| None))?;
            Ok(item)
        })).into_iter().filter_map(|x| x).collect::<Vec<_>>();
        c.next(is(symbol, '}'))?;
        Ok(opts)
    });

    let options = c.next(options_parser)
        .or_else(|_| c.next(is(symbol, ';')).map(|_| vec![]))?;

    Ok(options)
}));

// rpc = "rpc" rpcName "(" [ "stream" ] messageType ")" "returns" "(" [ "stream"
// ]       messageType ")" (( "{" { option | emptyStatement } "}" ) | ";" )
parser!(rpc<&str, RPC> => seq!(c => {
    c.next(is(ident, "rpc"))?;
    let name = c.next(rpcName)?;
    c.next(is(symbol, '('))?;

    let is_stream = map(
        opt(is(ident, "stream")),
        |v| v.map(|_| true).unwrap_or(false));

    let req_stream = c.next(&is_stream)?;
    let req_type = c.next(messageType)?;
    c.next(is(symbol, ')'))?;
    c.next(is(ident, "returns"))?;
    c.next(is(symbol, '('))?;

    let res_stream = c.next(is_stream)?;
    let res_type = c.next(messageType)?;
    c.next(is(symbol, ')'))?;

    let options = c.next(options_body)?;

    Ok(RPC { name, req_type, req_stream, res_type, res_stream, options })
}));

// Proto 2 only
// stream = "stream" streamName "(" messageType "," messageType ")" (( "{"
// { option | emptyStatement } "}") | ";" )
parser!(stream<&str, Stream> => seq!(c => {
    c.next(is(ident, "stream"))?;
    let name = c.next(streamName)?;
    c.next(is(symbol, '('))?;
    let input_type = c.next(messageType)?;
    c.next(is(symbol, ','))?;
    let output_type = c.next(messageType)?;
    c.next(is(symbol, ')'))?;
    let options = c.next(options_body)?;
    Ok(Stream { name, input_type, output_type, options })
}));

pub enum ProtoItem {
    Import(Import),
    Option(Opt),
    Package(String),
    TopLevelDef(TopLevelDef),
    None,
}

// Proto 2 and 3
// proto = syntax { import | package | option | topLevelDef | emptyStatement }
parser!(proto<&str, Proto> => seq!(c => {
    let s = c.next(syntax)?;
    // TODO: If no syntax is available, default to proto 2
    let body = c.many(alt!(
        map(import, |v| ProtoItem::Import(v)),
        map(package, |v| ProtoItem::Package(v)),
        map(option, |v| ProtoItem::Option(v)),
        map(top_level_def, |v| ProtoItem::TopLevelDef(v)),
        map(emptyStatement, |v| ProtoItem::None)
    ));

    let mut p = Proto {
        syntax: s,
        package: String::new(),
        imports: vec![],
        options: vec![],
        definitions: vec![]
    };

    let mut has_package = false;
    for item in body.into_iter() {
        match item {
            ProtoItem::Import(i) => { p.imports.push(i); },
            ProtoItem::Option(o) => { p.options.push(o); },
            ProtoItem::Package(s) => {
                // A proto file should only up to one package declaration.
                if has_package {
                    return Err(err_msg(
                        "Multiple package declarations in file"));
                }

                has_package = true;
                p.package = s;
            },
            ProtoItem::TopLevelDef(d) => { p.definitions.push(d); },
            ProtoItem::None => {}
        };
    }

    // TODO: Should now be at the end of the file
    Ok(p)
}));

pub fn parse_proto(file: &str) -> Result<Proto> {
    let (v, _) = complete(seq!(c => {
        let v = c.next(proto)?;
        c.next(Token::parse_padding)?;
        Ok(v)
    }))(file)?;

    Ok(v)
}

// TODO: Proto3 has no extend
// topLevelDef = message | enum | extend | service
parser!(top_level_def<&str, TopLevelDef> => alt!(
    map(message, |m| TopLevelDef::Message(m)),
    map(enum_, |e| TopLevelDef::Enum(e)),
    map(extend, |e| TopLevelDef::Extend(e)),
    map(service, |s| TopLevelDef::Service(s))
));
