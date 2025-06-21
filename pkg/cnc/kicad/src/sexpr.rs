
/*
https://dev-docs.kicad.org/en/file-formats/sexpr-intro/
https://dev-docs.kicad.org/en/components/sexpr/index.html

- Strings are double quoated and UTF-8 encoded
- All tokens are lowercase and don't whitespace or special characters aside from '_'
- Tokens may have attributes

*/

use parsing::*;
use common::errors::*;
use protobuf_core::tokenizer::parse_str_lit_inner;

#[derive(Debug, Clone)]
pub enum SExpr {
    Unquoted(String),
    Quoted(String),
    List(Vec<SExpr>)
}

impl SExpr {
    pub fn parse(input: &str) -> Result<Self> {
        Ok(parsing::complete(Self::parse_partial)(input)?.0)
    }

    parser!(parse_list<&str, Vec<Self>> => seq!(c => {
        c.next(opt(whitespace))?;
        c.next(tag("("))?;
        c.next(opt(whitespace))?;

        let mut items = vec![];
        loop {
            let item = c.next(opt(Self::parse_partial))?;

            if let Some(item) = item {
                items.push(item);
            } else {
                break;
            }

            // Items must be delimited by whitespace.
            if c.next(opt(whitespace))?.is_none() && c.next(opt(peek(tag("("))))?.is_none() {
                break;
            }
        }

        c.next(tag(")"))?;
        c.next(opt(whitespace))?;

        Ok(items)
    }));

    parser!(parse_partial<&str, Self> => seq!(c => {
        c.next(alt!(
            map(Self::parse_list, |v| Self::List(v)),
            map(quoted_item, |v| Self::Quoted(v)),
            // Must be parsed last since this will accept any first character.
            map(unquoted_item, |v| Self::Unquoted(v))
        ))
    }));
}

parser!(whitespace<&str, ()> => map(
    take_while1(|c: char| c.is_whitespace()),
    |_| ()
));

parser!(unquoted_item<&str, String> => seq!(c => {
    let s: &str = c.next(take_while1(|c: char| !c.is_whitespace() && c != '(' && c != ')'))?;
    if s.contains('"') {
        return Err(err_msg("Unquoted value can't contain quotes"));
    }

    Ok(s.to_string())
}));

parser!(quoted_item<&str, String> => seq!(c => {
    c.next(tag("\""))?;
    let out = String::from_utf8(c.next(parse_str_lit_inner('"'))?)?;
    c.next(tag("\""))?;
    Ok(out)
}));


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_parse() -> Result<()> {
        let e = SExpr::parse("(hello world \"it\\\"s\" (1 2 3))")?;
        println!("{:#?}", e);
        Ok(())
    }
}

