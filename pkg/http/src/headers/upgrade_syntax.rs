use parsing::*;
use parsing::ascii::*;

use crate::headers::upgrade::*;
use crate::message_syntax::parse_token;

// RFC 7230: 6.7
//
// `protocol = protocol-name [ "/" protocol-version ]`
parser!(pub parse_protocol<Protocol> => {
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
