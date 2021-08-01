// Common parsing rules used in other places.

use common::bytes::Bytes;
use parsing::*;

// RFC 7230: Section 3.2.3
//
// `BWS = OWS`
parser!(pub parse_bws<Bytes> => parse_ows);

// RFC 7230: Section 3.2.3
//
// Optional whitespace
// `OWS = *( SP / HTAB )`
parser!(pub parse_ows<Bytes> => {
    take_while(|i| is_sp(i) || is_htab(i))
});

// RFC 7230: Section 3.2.3
//
// Required whitespace
// `RWS = 1*( SP / HTAB )`
parser!(pub parse_rws<Bytes> => {
    take_while1(|i| is_sp(i) || is_htab(i))
});

pub fn is_sp(i: u8) -> bool {
    i == (' ' as u8)
}
pub fn sp(input: Bytes) -> ParseResult<u8> {
    like(is_sp)(input)
}

pub fn is_htab(i: u8) -> bool {
    i == ('\t' as u8)
}

// Visible USASCII character.
pub fn is_vchar(i: u8) -> bool {
    i >= 0x21 && i <= 0x7e
}

parser!(pub parse_crlf<()> => tag(b"\r\n"));
