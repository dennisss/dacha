use std::collections::HashMap;

use common::errors::*;
use parsing::*;

use crate::value::Value;

pub fn parse_string(allow_single_quote: bool) -> impl Fn(&str) -> Result<(String, &str)> {
    seq!(c => {
        let quote = c.next(one_of(if allow_single_quote { "\"'" } else { "\"" }))?;

        let mut s = String::new();
        while let Some(v) = c.next(|v| parse_character(v, quote))? {
            s.push(v);
        }

        Ok(s)
    })
}

// Will return None if we hit the end quote.
pub(crate) fn parse_character(input: &str, quote: char) -> Result<(Option<char>, &str)> {
    seq!(c => {
        let mut v: char = c.next(like(|_| true))?;
        if (v as u32) < 0x20 {
            return Err(err_msg("Unallowed character value"));
        }

        if v == quote {
            return Ok(None);
        }

        if v == '\\' {
            let escape_type: char = c.next(like(|_| true))?;

            match escape_type {
                '\\' | '/' => { v = escape_type; }
                'b' => { v = '\x08'; }
                'f' => { v = '\x0C'; }
                'n' => { v = '\n'; }
                'r' => { v = '\r'; }
                't' => { v = '\t'; }
                'u' => {
                    let hex = c.next(take_exact(4))?;
                    let n = u16::from_str_radix(hex, 16)?;
                    v = char::from_u32(n as u32).unwrap();
                }
                _ => {
                    if escape_type == quote {
                        v = quote;
                    } else {
                        return Err(err_msg("Unsupported escape type"));
                    }
                }
            }
        }

        Ok(Some(v))
    })(input)
}

regexp!(NUMBER => "^-?(?:[1-9][0-9]+|[0-9])(?:\\.[0-9]+)?(?:[eE][+\\-]?[0-9]+)?");

pub fn parse_number(input: &str) -> ParseResult<f64, &str> {
    if let Some(m) = NUMBER.exec(input) {
        let (num_str, rest) = input.split_at(m.last_index());

        let num = num_str.parse::<f64>()?;
        return Ok((num, rest));
    }

    Err(err_msg("Not a number"))
}

parser!(parse_whitespace<&str, ()> => seq!(c => {
    c.next(many(one_of(" \n\r\t")))?;
    Ok(())
}));

#[cfg(test)]
mod tests {

    #[test]
    fn regexp_usage_test() {
        println!("NUMBER REGEXP {}", super::NUMBER.estimated_memory_usage());
    }
}
