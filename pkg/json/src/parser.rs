use std::collections::HashMap;

use common::errors::*;
use parsing::*;

use crate::value::Value;

parser!(pub parse_json<&str, Value> => {
    parse_element
});

parser!(parse_value<&str, Value> => alt!(
    parse_object,
    parse_array,
    map(parse_string, |s| Value::String(s)),
    map(parse_number, |v| Value::Number(v)),
    map(tag("true"), |_| Value::Bool(true)),
    map(tag("false"), |_| Value::Bool(false)),
    map(tag("null"), |_| Value::Null)
));

parser!(parse_object<&str, Value> => seq!(c => {
    c.next(tag("{"))?;
    c.next(parse_whitespace)?;

    let mut obj = HashMap::new();
    for (key, value) in c.next(delimited(parse_member, tag(",")))? {
        if obj.contains_key(&key) {
            return Err(format_err!("Duplicate key in object: {:?}", key));
        }
        
        obj.insert(key, value);
    }

    c.next(tag("}"))?;
    Ok(Value::Object(obj))
}));

parser!(parse_member<&str, (String, Value)> => seq!(c => {
    c.next(parse_whitespace)?;
    let key = c.next(parse_string)?;
    c.next(parse_whitespace)?;
    c.next(tag(":"))?;
    let value = c.next(parse_element)?;
    Ok((key, value))
}));

parser!(parse_array<&str, Value> => seq!(c => {
    c.next(tag("["))?;
    c.next(parse_whitespace)?;

    let mut arr = vec![];
    for el in c.next(delimited(parse_element, tag(",")))? {
        arr.push(el);
    }

    c.next(tag("]"))?;
    Ok(Value::Array(arr))
}));

parser!(parse_element<&str, Value> => seq!(c => {
    c.next(parse_whitespace)?;
    let value = c.next(parse_value)?;
    c.next(parse_whitespace)?;
    Ok(value)
}));

parser!(parse_string<&str, String> => seq!(c => {
    c.next(tag("\""))?;

    let mut s = String::new();
    while let Some(v) = c.next(opt(parse_character))? {
        s.push(v);
    }

    c.next(tag("\""))?;

    Ok(s)
}));

parser!(parse_character<&str, char> => seq!(c => {
    let mut v: char = c.next(like(|_| true))?;
    if (v as u32) < 0x20 || v == '"' {
        return Err(err_msg("Unallowed character value"));
    }

    if v == '\\' {
        let escape_type: char = c.next(like(|_| true))?;

        match escape_type {
            '"' | '\\' | '/' => { v = escape_type; }
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
                return Err(err_msg("Unsupported escape type"));
            }
        }
    }

    Ok(v)
}));


regexp!(NUMBER => "^-?(?:[1-9][0-9]+|[0-9])(?:\\.[0-9]+)?(?:[eE][+\\-]?[0-9]+)?");

fn parse_number(input: &str) -> ParseResult<f64, &str> {
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