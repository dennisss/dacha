use common::errors::*;
use parsing::*;

#[derive(Debug)]
pub enum Element<'a> {
    Attribute {
        key: &'a str,
        value: Option<&'a str>,
    },
    Field {
        key: &'a str,
        value: Option<&'a str>,
    },
    EndOfLine,
}

impl Element<'_> {
    /// NOTE: Parsing an empty string will return Element::EndOfLine so the
    /// caller must check if the string is empty before repeating to call this.
    pub fn parse_next(input: &str) -> Result<(Element, &str)> {
        alt!(
            Element::parse_attribute,
            Element::parse_field,
            Element::parse_eol
        )(input)
    }

    // Attribute = '[' OWS KeyValue OWS ']' EOL
    parser!(parse_attribute<&str, Element> => seq!(c => {
        c.next(one_of("["))?;
        c.next(opt(Token::parse_whitespace))?;
        let (key, value) = c.next(Element::parse_key_value)?;
        c.next(opt(Token::parse_whitespace))?;
        c.next(one_of("]"))?;
        c.next(Element::parse_eol)?;

        Ok(Element::Attribute { key, value })
    }));

    // Field = KeyValue EOL
    parser!(parse_field<&str, Element> => seq!(c => {
        let (key, value) = c.next(Element::parse_key_value)?;
        c.next(Element::parse_eol)?;
        Ok(Element::Field { key, value })
    }));

    // KeyValue = Text ( OWS '=' (OWS Text)* )*
    parser!(parse_key_value<&str, (&str, Option<&str>)> => seq!(c => {
        let key = c.next(Token::parse_text)?;
        let value = c.next(opt(seq!(c => {

            c.next(opt(Token::parse_whitespace))?;
            c.next(one_of("="))?;

            let v = c.next(opt(seq!(c => {
                c.next(opt(Token::parse_whitespace))?;
                c.next(Token::parse_text)
            })))?.unwrap_or("");

            Ok(v)
        })))?;

        Ok((key, value))
    }));

    parser!(parse_eol<&str, Element> => seq!(c => {
        c.next(opt(Token::parse_whitespace))?;
        c.next(alt!(
            map(Token::parse_new_line, |_| ()),
            map(Token::parse_comment, |_| ()),
            Token::parse_empty
        ))?;

        Ok(Element::EndOfLine)
    }));
}

enum Token<'a> {
    Comment(&'a str),
    Symbol(char),
    Whitespace(&'a str),
    NewLine,
    Text(&'a str),
}

impl Token<'_> {
    /*
    /// Parses the next token while skipping non-semantic tokens like whitespace or
    /// comments.
    pub fn parse_semantic<'a>(mut input: &'a str) -> Result<(Option<Token<'a>>, &'a str)> {
        loop {
            if input.len() == 0 {
                return Ok((None, input));
            }

            let r: Result<(Token, &'a str)> = alt!(
                map(Token::parse_raw_comment, |v| Token::Comment(v)),
                map(Token::parse_raw_symbol, |v| Token::Symbol(v)),
                map(Token::parse_raw_whitespace, |v| Token::Whitespace(v)),
                map(Token::parse_raw_text, |v| Token::Text(v))
            )(input);

            let (tok, rest) = r?;

            match tok {
                Token::Text(_) | Token::Symbol(_) | Token::NewLine => { return Ok((Some(tok), rest)); },
                Token::Symbol(_) => { return Ok((Some(tok), rest)) }
                Token::Comment(_) | Token::Whitespace(_) => {
                    input = rest;
                }
            }
        }
    }

    pub fn parse_text<'a>(input: &'a str) -> Result<(&'a str, &'a str)> {
        let (tok, rest) = Token::parse_semantic(input)?;
        match tok {
            Some(Token::Text(v)) => { return Ok((v, rest)); },
            _ => { return Err(err_msg("Not text!")) }
        }
    }

    pub fn parse_symbol<'a>(c: char) -> impl Parser<char, &'a str> {
        move |input: &'a str| {
            let (tok, rest) = Token::parse_semantic(input)?;
            match tok {
                Some(Token::Symbol(c)) => { return Ok((c, rest)); },
                _ => { return Err(err_msg("Not the right symbol!")) }
            }
        }
    }

    pub fn parse_empty<'a>(input: &'a str) -> Result<((), &'a str)> {
        let (tok, rest) = Token::parse_semantic(input)?;
        match tok {
            None => { return Ok(((), rest)); },
            _ => { return Err(err_msg("Not empty!")) }
        }
    }

    pub fn parse_new_line<'a>(input: &'a str) -> Result<((), &'a str)> {
        let (tok, rest) = Token::parse_semantic(input)?;
        match tok {
            Some(Token::NewLine) => { return Ok(((), rest)); },
            _ => { return Err(err_msg("Not empty!")) }
        }
    }
    */

    parser!(parse_comment<&str, &str> => seq!(c => {
        c.next(tag("#"))?;
        let end_marker = tag("\n");
        let inner = c.next(take_until(&end_marker))?;
        c.next(end_marker)?;
        Ok(inner)
    }));

    parser!(parse_symbol<&str, char> => one_of("[]="));

    parser!(parse_whitespace<&str, &str> => slice(many1(one_of("\t\r "))));

    parser!(parse_new_line<&str, char> => one_of("\n"));

    parser!(parse_text<&str, &str> => take_while1(|c: char| c.is_alphanumeric()));

    fn parse_empty(input: &str) -> Result<((), &str)> {
        if input.len() == 0 {
            return Ok(((), input));
        } else {
            return Err(err_msg("Not empty"));
        }
    }
}
