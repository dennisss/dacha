use std::collections::HashMap;

use base_util::zip_all::ZipAllIterator;

pub fn extract_path_params(path: &str, pattern: &str) -> Option<HashMap<String, String>> {
    // TODO: Ensure that the path is first normalized

    let path_parts = path.split('/');
    let pattern_parts = pattern.split('/');

    let iter = ZipAllIterator::new(path_parts, pattern_parts);

    let mut params = HashMap::default();

    for (path_part, pattern_part) in iter {
        let path_part = match path_part {
            Some(v) => v,
            None => return None,
        };

        let pattern_part = match pattern_part {
            Some(v) => v,
            None => return None,
        };

        if let Some(param_name) = pattern_part.strip_prefix(':') {
            params.insert(param_name.to_string(), path_part.to_string());
        } else if path_part != pattern_part {
            return None;
        }
    }

    Some(params)
}

// NOTE: Currently assuming the input is ASCII before and after decoding.
pub fn decode_uri_component(value: &str) -> String {
    let mut out = String::new();

    let mut i = 0;

    let value = value.as_bytes();
    
    while i < value.len() {
        let mut b = value[i];
        i += 1;

        if b == b'%' && i + 1 < value.len() {
            if let Ok(s) = std::str::from_utf8(&value[i..(i + 2)]) {
                if let Ok(v) = u8::from_str_radix(s, 16) {
                    b = v;
                    i += 2;
                }
            }
        }

        out.push(b as char);
    }

    out
}