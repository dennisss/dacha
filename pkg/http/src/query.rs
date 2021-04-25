// Utilities for dealing with the form parameters encoded in a URL's query string or form
// body also known as 'application/x-www-form-urlencoded'.
//
// The specificication is defined in:
// https://url.spec.whatwg.org/#application/x-www-form-urlencoded

use std::collections::HashMap;

use common::errors::*;
use parsing::opaque::OpaqueString;

// /// Map based storage of query parameters.
// /// This is efficient for lookup but ignores any ordering between values with different names. 
// pub struct QueryParams {
//     params: HashMap<OpaqueString, Vec<OpaqueString>>,
// }

pub struct QueryParamsParser<'a> {
    input: &'a [u8],
}

impl<'a> QueryParamsParser<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self { input }
    }

    fn decode_percent_encoded(&mut self) -> Option<u8> {
        if self.input.len() < 2 {
            return None;
        }
        
        let s = match std::str::from_utf8(&self.input[0..2]) {
            Ok(s) => s,
            Err(_) => { return None; }
        };

        match u8::from_str_radix(s, 16) {
            Ok(v) => {
                self.input = &self.input[2..];
                Some(v)
            },
            Err(_) => None
        }
    }
}

impl std::iter::Iterator for QueryParamsParser<'_> {
    type Item = (OpaqueString, OpaqueString);

    fn next(&mut self) -> Option<Self::Item> {
        let mut name = vec![];
        let mut value = vec![];
        let mut parsing_value = false;

        while !self.input.is_empty() && self.input[0] == b'&' {
            self.input = &self.input[1..];
        }

        if self.input.is_empty() {
            return None;
        }

        while !self.input.is_empty() {
            let mut byte = self.input[0];
            self.input = &self.input[1..];

            if byte == b'=' {
                if !parsing_value {
                    parsing_value = true;
                    continue;
                }
            } else if byte == b'&' {
                break;
            }
            
            if byte == b'+' {
                byte = b' ';
            } else if byte == b'%' {
                if let Some(decoded) = self.decode_percent_encoded() {
                    byte = decoded;
                }
            }

            if parsing_value {
                value.push(byte);
            } else {
                name.push(byte);
            }
        }

        Some((OpaqueString::from(name), OpaqueString::from(value)))
    }
}

#[cfg(test)]
mod tests{
    use super::*;

    #[test]
    fn parser_test() {
        // TODO: Distinguish between 'name' and 'name='?
        let input = b"&hello=wor=ld&value=123 +go&&=&name&encoded=%333r%ZZ%";
        let raw_expected_outputs: &[(&[u8], &[u8])] = &[
            (b"hello", b"wor=ld"),
            (b"value", b"123  go"),
            (b"", b""),
            (b"name", b""),
            (b"encoded", b"33r%ZZ%")
        ];

        let expected_outputs = raw_expected_outputs.iter()
            .map(|(k, v)| (OpaqueString::from(*k), OpaqueString::from(*v))).collect::<Vec<_>>();

        let outputs = QueryParamsParser::new(input).collect::<Vec<_>>();
        assert_eq!(outputs, expected_outputs);
    }

}
