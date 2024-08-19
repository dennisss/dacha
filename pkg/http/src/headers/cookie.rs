use common::errors::*;

use crate::header::{Headers, COOKIE};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
}

/// https://datatracker.ietf.org/doc/html/rfc6265
///
/// Note that we restrict ourselves to UTF-8 cookie names and values.
pub fn parse_cookie_header(headers: &Headers) -> Result<Vec<Cookie>> {
    // Only zero or one Cookie headers are allowed on a request.
    let header = match headers.get_one(COOKIE)? {
        Some(v) => v,
        None => return Ok(vec![]),
    };

    let value = header.value.to_utf8_str()?;
    parse_cookie_string(value)
}

pub fn parse_cookie_string(value: &str) -> Result<Vec<Cookie>> {
    let mut out = vec![];

    /*
    TODO: Need size limits.

    Per the RFC:
    o  At least 4096 bytes per cookie (as measured by the sum of the
        length of the cookie's name, value, and attributes).
    o  At least 50 cookies per domain.
    o  At least 3000 cookies total.
    */

    for pair in value.split(";") {
        let (k, v) = pair
            .split_once("=")
            .ok_or_else(|| err_msg("Missing = in cookie tuple"))?;

        out.push(Cookie {
            name: k.trim().to_string(),
            value: v.trim().to_string(),
        });
    }

    Ok(out)
}

// pub fn parse_set_cookie_headers(headers: &Headers)

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn parse_cookie_string_test() {
        let res = parse_cookie_string("yummy_cookie=choco; tasty_cookie=strawberry").unwrap();

        assert_eq!(
            res,
            vec![
                Cookie {
                    name: "yummy_cookie".into(),
                    value: "choco".into(),
                },
                Cookie {
                    name: "tasty_cookie".into(),
                    value: "strawberry".into(),
                },
            ]
        );
    }
}
