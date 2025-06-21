use common::errors::*;
use reflection::{ParsingTypeHint, PrimitiveValue};

use crate::sexpr::SExpr;

/// Interprets a 
pub struct SExprReader<'a> {
    value: &'a SExpr
}

impl<'a> SExprReader<'a> {
    pub fn new(value: &'a SExpr) -> Self {
        Self { value }
    }
}

impl<'a> reflection::ValueReader<'a> for SExprReader<'a> {
    fn parse<T: reflection::ParseFromValue<'a>>(self) -> Result<T> {

        match self.value {
            SExpr::Quoted(s) => {
                todo!()
            }
            SExpr::Unquoted(s) => {
                todo!()
            }
            SExpr::List(items) => {
                if items.len() < 1 {
                    return Err(err_msg("Expected at least a token in the object."))
                }

                // TODO: Check against T::parsing_typename() when parsing objects?
                let token = parse_token(&items[0])?;

                let hint = T::parsing_hint().ok_or_else(|| err_msg("Need a type hint"))?;
                match hint {
                    // TODO: We currently assume it is a list of objects
                    ParsingTypeHint::Object | ParsingTypeHint::List => {
                        let attrs = &items[1..];
                        T::parse_from_object(ObjectParser { attrs })
                    }
                    _ => {
                        // Got an attribute of an outer object.

                        if items.len() != 2 {
                            println!("{:?}", items);

                            return Err(err_msg("Expected only two items in attr"));
                        }

                        match &items[1] {
                            SExpr::Quoted(s) => {
                                T::parse_from_primitive(reflection::PrimitiveValue::String(s.clone()))
                            }
                            SExpr::Unquoted(s) => {
                                parse_from_string(&s)
                            }
                            _ => {
                                todo!()
                            }
                        }
                    }
                }
            }
        }
    }
}


fn parse_token(e: &SExpr) -> Result<&str> {
    let token = match e {
        SExpr::Unquoted(s) => s.as_str(),
        _ => return Err(err_msg("Bad token format"))
    };

    for c in token.chars() {
        let valid = c == '_' || (c.is_ascii_alphanumeric() && c.is_ascii_lowercase());
        if !valid {
            return Err(err_msg("Invlaid object token"));
        }
    }

    Ok(token)
}

// TODO: Dedup with the XML code.
fn parse_from_string<'data, T: reflection::ParseFromValue<'data>>(s: &'data str) -> Result<T> {
    let primitive = match T::parsing_hint() {
        Some(v) => match v {
            ParsingTypeHint::Null
            | ParsingTypeHint::String
            | ParsingTypeHint::Object
            | ParsingTypeHint::List => PrimitiveValue::Str(s),
            // TODO: Verify that the string formats used by these parsers match what is allowed in
            // the XML schema spec.
            ParsingTypeHint::Bool => todo!(),
            ParsingTypeHint::I8 => PrimitiveValue::I8(s.parse()?),
            ParsingTypeHint::U8 => PrimitiveValue::U8(s.parse()?),
            ParsingTypeHint::I16 => PrimitiveValue::I16(s.parse()?),
            ParsingTypeHint::U16 => PrimitiveValue::U16(s.parse()?),
            ParsingTypeHint::I32 => PrimitiveValue::I32(s.parse()?),
            ParsingTypeHint::U32 => PrimitiveValue::U32(s.parse()?),
            ParsingTypeHint::I64 => PrimitiveValue::I64(s.parse()?),
            ParsingTypeHint::U64 => PrimitiveValue::U64(s.parse()?),
            ParsingTypeHint::ISize => PrimitiveValue::ISize(s.parse()?),
            ParsingTypeHint::USize => PrimitiveValue::USize(s.parse()?),
            ParsingTypeHint::F32 => PrimitiveValue::F32(s.parse()?),
            ParsingTypeHint::F64 => PrimitiveValue::F64(s.parse()?),
        },
        None => PrimitiveValue::Str(s),
    };

    T::parse_from_primitive(primitive)
}

struct ObjectParser<'a> {
    attrs: &'a [SExpr],
}

impl<'a> reflection::ObjectIterator<'a> for ObjectParser<'a> {
    type ValueReaderType = SExprReader<'a>;

    fn next_field(&mut self) -> Result<Option<(String, Self::ValueReaderType)>> {
        if self.attrs.len() == 0 {
            return Ok(None);
        }

        let attr = &self.attrs[0];
        self.attrs = &self.attrs[1..];

        let list = match attr {
            SExpr::List(v) => &v[..],
            _ => return Err(err_msg("Expected each attr to be a list"))
        };

        if list.len() < 1 {
            println!("{:#?}", list);

            return Err(err_msg("Expected each attr to have a token list"));
        }

        let token = parse_token(&list[0])?;

        Ok(Some((token.to_string(), SExprReader::new(attr))))
    }
}
