use std::fmt::Write;

use common::errors::*;
use parsing::*;

use crate::common_syntax::parse_ows;
use crate::header::{Headers, CONTENT_TYPE};
use crate::message_syntax::parse_quoted_string;
use crate::message_syntax::parse_token;

pub fn parse_content_type(headers: &Headers) -> Result<Option<MediaType>> {
    let mut h_iter = headers.find(CONTENT_TYPE);
    let h = if let Some(h) = h_iter.next() {
        h
    } else {
        return Ok(None);
    };
    if !h_iter.next().is_none() {
        return Err(err_msg("More than one content-type"));
    }

    let (typ, _) = complete(MediaType::parse)(h.value.to_bytes())?;
    Ok(Some(typ))
}

// NOTE: All of this is case-insensitive and will be stored in ascii lower case.
pub struct MediaType {
    pub typ: String,
    pub subtype: String,
    pub suffix: Option<String>,
    pub params: Vec<Parameter>,
}

impl MediaType {
    parser!(parse<MediaType> => {
        seq!(c => {
            let typ = c.next(parse_token)?.to_string().to_ascii_lowercase();
            c.next(one_of("/"))?;
            let mut subtype = c.next(parse_token)?.to_string().to_ascii_lowercase();
            let mut suffix = None;
            if let Some((a, b)) = subtype.split_once('+') {
                suffix = Some(b.to_string());
                subtype = a.to_string();
            }

            let params = c.many(seq!(c => {
                c.next(parse_ows)?;
                c.next(one_of(";"))?;
                c.next(parse_ows)?;
                c.next(Parameter::parse)
            }));

            Ok(MediaType {
                typ, subtype, suffix, params
            })
        })
    });

    pub fn to_string(&self) -> String {
        let mut out = format!("{}/{}", self.typ, self.subtype);
        if let Some(suffix) = &self.suffix {
            write!(&mut out, "+{}", suffix).unwrap();
        }

        for param in &self.params {
            write!(&mut out, "; {}", param.to_string()).unwrap();
        }

        out
    }
}

pub struct Parameter {
    pub name: String,
    // NOTE: Case sentitive detepending on the parameter name.
    pub value: String,
}

impl Parameter {
    parser!(parse<Parameter> => {
        seq!(c => {
            let name = c.next(parse_token)?.to_string().to_ascii_lowercase();
            c.next(one_of("="))?;
            let value = c.next(alt!(
                map(parse_token, |t| t.to_string()),
                map(parse_quoted_string, |t| t.to_string())
            ))?;
            Ok(Parameter { name, value })
        })
    });

    pub fn to_string(&self) -> String {
        // TODO: Properly encode quoted characters
        format!("{}=\"{}\"", self.name, self.value)
    }
}

#[cfg(test)]
mod tests {
    use common::bytes::Bytes;

    use super::*;

    #[test]
    fn works() -> Result<()> {
        let (t, _) = complete(MediaType::parse)(Bytes::from("application/grpc+proto"))?;
        assert_eq!(t.typ, "application");
        assert_eq!(t.subtype, "grpc");
        assert_eq!(t.suffix, Some(String::from("proto")));
        Ok(())
    }
}
