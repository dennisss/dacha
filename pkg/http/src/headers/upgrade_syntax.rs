use common::errors::*;
use parsing::*;
use parsing::ascii::*;

use crate::{header_syntax::comma_delimited, headers::upgrade::*};
use crate::message_syntax::parse_token;
use crate::header::{Headers, UPGRADE};

// RFC 7230: 6.7
//
// `protocol = protocol-name [ "/" protocol-version ]`
parser!(parse_protocol<Protocol> => {
    seq!(c => {
        let name = c.next(parse_protocol_name)?;
        let version = c.next(opt(seq!(c => {
            c.next(tag(b"/"))?;
            c.next(parse_protocol_version)
        })))?;

        Ok(Protocol {
            name, version
        })
    })
});

// RFC 7230: 6.7
//
// `protocol-name = token`
parser!(parse_protocol_name<AsciiString> => parse_token);

// RFC 7230: 6.7
//
// `protocol-version = token`
parser!(parse_protocol_version<AsciiString> => parse_token);


/// RFC 7230: 6.7
///
/// `Upgrade = 1#protocol`
pub fn parse_upgrade(headers: &Headers) -> Result<Vec<Protocol>> {
    let mut out = vec![];

    for header in headers.find(UPGRADE) {
        let (v, _) = complete(comma_delimited(parse_protocol, 1, 10))(header.value.to_bytes())?;
        out.extend(v.into_iter());
    }

    Ok(out)
}