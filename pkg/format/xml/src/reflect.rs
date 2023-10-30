use common::errors::*;
use reflection::{ParsingTypeHint, PrimitiveValue};

use crate::{Document, Element, Node};

/*
Body parsing modes:

- List, treat each child separately

- If hinted as a 'String'
    - Assert the

*/

pub const XML_CONTENT_KEY: &'static str = "$content";

pub struct ElementParser<'data> {
    element: &'data Element,
    root: bool,
}

impl<'data> ElementParser<'data> {
    pub fn new(element: &'data Element, root: bool) -> Self {
        Self { element, root }
    }
}

impl<'data> reflection::ValueReader<'data> for ElementParser<'data> {
    fn parse<T: reflection::ParseFromValue<'data>>(self) -> Result<T> {
        if self.root {
            let typename = T::parsing_typename()
                .ok_or_else(|| err_msg("Expected XML element types to have a well defined name"))?;

            if self.element.name != typename {
                return Err(format_err!(
                    "Element name mismatch: {} vs {}",
                    typename,
                    self.element.name
                ));
            }
        }

        T::parse_from_object(ElementKeyIterator {
            attrs: self.element.attributes.iter(),
            content: &self.element.content,
            end: false,
        })
    }
}

struct ElementKeyIterator<'data> {
    attrs: std::collections::hash_map::Iter<'data, String, String>,
    content: &'data [Node],
    end: bool,
}

impl<'data> reflection::ObjectIterator<'data> for ElementKeyIterator<'data> {
    type ValueReaderType = ElementKeyValueParser<'data>;

    fn next_field(&mut self) -> Result<Option<(String, Self::ValueReaderType)>> {
        if let Some((key, value)) = self.attrs.next() {
            return Ok(Some((
                key.clone(),
                ElementKeyValueParser::Attribute(value.as_str()),
            )));
        }

        if !self.end {
            self.end = true;

            if self.content.is_empty() {
                return Ok(None);
            }

            return Ok(Some((
                XML_CONTENT_KEY.to_string(),
                ElementKeyValueParser::Content(&self.content),
            )));
        }

        Ok(None)
    }
}

enum ElementKeyValueParser<'data> {
    Attribute(&'data str),
    Content(&'data [Node]),
}

impl<'data> reflection::ValueReader<'data> for ElementKeyValueParser<'data> {
    fn parse<T: reflection::ParseFromValue<'data>>(self) -> Result<T> {
        match self {
            ElementKeyValueParser::Attribute(data) => parse_from_string(data),
            ElementKeyValueParser::Content(nodes) => {
                let hint = T::parsing_hint().unwrap_or(reflection::ParsingTypeHint::List);

                match hint {
                    reflection::ParsingTypeHint::List => {
                        // parse as a list of different nodes.

                        todo!()
                    }
                    reflection::ParsingTypeHint::String => {
                        // TODO: Ignore comments.
                        if nodes.len() != 1 {
                            return Err(err_msg("Expected exactly one string node"));
                        }

                        match &nodes[0] {
                            Node::Text(text) => parse_from_string(text.as_str()),
                            _ => Err(err_msg("Wrong node text. Wanted text")),
                        }
                    }
                    reflection::ParsingTypeHint::Object => {
                        T::parse_from_object(ElementContentObjectIterator {
                            content: nodes,
                            next_index: 0,
                        })
                    }
                    _ => Err(format_err!("Can not parse XML content as {:?}", hint)),
                }
            }
        }
    }
}

/// Calls T::parse_from_primitive for XML values.
///
/// Because XML does not natively differentiate between string|numeric|bool
/// types, this needs to be done at a higher level.
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

    T::parse_from_primitive(reflection::PrimitiveValue::Str(s))
}

struct ElementContentObjectIterator<'data> {
    content: &'data [Node],
    next_index: usize,
}

impl<'data> reflection::ObjectIterator<'data> for ElementContentObjectIterator<'data> {
    type ValueReaderType = ElementParser<'data>;

    fn next_field(&mut self) -> Result<Option<(String, Self::ValueReaderType)>> {
        loop {
            if self.next_index >= self.content.len() {
                return Ok(None);
            }

            let i = self.next_index;
            self.next_index += 1;

            match &self.content[i] {
                Node::Text(v) => {
                    if !v.trim_start().is_empty() {
                        return Err(format_err!("Unexpected text: {:?}", v));
                    }
                }
                Node::Element(v) => {
                    return Ok(Some((v.name.to_string(), ElementParser::new(v, false))));
                }
                Node::Comment(_) => {
                    // Ignore it
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::parse_document;

    use reflection::ParseFrom;

    use super::*;

    #[derive(Parseable, PartialEq, Debug)]
    struct Book {
        #[parse(name = "$content")]
        body: BookBody,
    }

    #[derive(Parseable, PartialEq, Debug)]
    struct BookBody {
        #[parse(name = "Title")]
        title: Title,

        #[parse(name = "Page")]
        pages: Vec<Page>,
    }

    #[derive(Parseable, PartialEq, Debug)]
    struct Title {
        #[parse(name = "$content")]
        value: String,
    }

    #[derive(Parseable, PartialEq, Debug)]
    struct Page {
        color: String,

        number: usize,

        #[parse(name = "$content")]
        text: String,
    }

    #[test]
    fn parse_book_test() -> Result<()> {
        let input = r#"<?xml version="1.0"?>
            <Book>
                <Title>Great Gatsby</Title>
                <Page number="1" color="red">My name is</Page>
                <Page number="2" color="blue">John</Page>
            </Book>
        "#;

        let (doc, rest) = parse_document(input)?;
        assert_eq!(rest, "");

        let book = Book::parse_from(ElementParser::new(&doc.root_element, true))?;

        assert_eq!(
            book,
            Book {
                body: BookBody {
                    title: Title {
                        value: "Great Gatsby".to_string()
                    },
                    pages: vec![
                        Page {
                            color: "red".to_string(),
                            number: 1,
                            text: "My name is".to_string()
                        },
                        Page {
                            color: "blue".to_string(),
                            number: 2,
                            text: "John".to_string()
                        },
                    ]
                }
            }
        );

        Ok(())
    }
}
