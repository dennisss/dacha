use std::collections::HashMap;

use common::errors::*;

pub fn parse_query(request: &http::Request) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    let data = match &request.head.uri.query {
        Some(v) => v.as_str(),
        None => return Ok(out),
    };

    let mut parser = http::query::QueryParamsParser::new(data.as_bytes());

    for (key, value) in parser.next() {
        let key = key.to_utf8_str()?.to_string();
        let value = value.to_utf8_str()?.to_string();
        if out.contains_key(&key) {
            return Err(err_msg("Duplicate key in query"));
        }

        out.insert(key, value);
    }

    Ok(out)
}