use common::errors::*;
use parsing::*;


pub struct Parameter {
    pub name: String,
    // NOTE: Case sentitive detepending on the parameter name.
    pub value: String,
}

parser!(parse_parameter<Parameter> => {
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

// NOTE: All of this is case-insensitive and will be stored in ascii lower case.
pub struct MediaType {
    pub typ: String,
    pub subtype: String,
    pub params: Vec<Parameter>,
}

parser!(parse_media_type<MediaType> => {
    seq!(c => {
        let typ = c.next(parse_token)?.to_string().to_ascii_lowercase();
        c.next(one_of("/"))?;
        let subtype = c.next(parse_token)?.to_string().to_ascii_lowercase();
        let params = c.many(seq!(c => {
            c.next(parse_ows)?;
            c.next(one_of(";"))?;
            c.next(parse_ows)?;
            c.next(parse_parameter)
        }));

        Ok(MediaType {
            typ, subtype, params
        })
    })
});

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

    let (typ, _) = complete(parse_media_type)(h.value.data.clone())?;
    Ok(Some(typ))
}