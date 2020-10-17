use common::errors::*;
use common::bytes::Bytes;
use parsing::*;
use parsing::iso::*;
use parsing::ascii::*;

// `BWS = OWS`
parser!(pub parse_bws<Bytes> => parse_ows);

// Optional whitespace
// `OWS = *( SP / HTAB )`
parser!(pub parse_ows<Bytes> => {
	take_while(|i| is_sp(i) || is_htab(i))
});

// Required whitespace
// `RWS = 1*( SP / HTAB )`
parser!(pub parse_rws<Bytes> => {
	take_while1(|i| is_sp(i) || is_htab(i))
});

pub fn is_sp(i: u8) -> bool { i == (' ' as u8) }
pub fn sp(input: Bytes) -> ParseResult<u8> { like(is_sp)(input) }

pub fn is_htab(i: u8) -> bool { i == ('\t' as u8) }

// Visible USASCII character.
pub fn is_vchar(i: u8) -> bool {
	i >= 0x21 && i <= 0x7e
}


parser!(pub parse_crlf<()> => tag(b"\r\n"));

// NOTE: This is strictly ASCII.
// `tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." /
//  "^" / "_" / "`" / "|" / "~" / DIGIT / ALPHA`
fn is_tchar(i: u8) -> bool {
	(i as char).is_ascii_alphanumeric() || i.is_one_of(b"!#$%&'*+-.^_`|~")
}

// NOTE: This is strictly ASCII.
// `token = 1*tchar`
pub fn parse_token(input: Bytes) -> ParseResult<AsciiString> {
	let (v, rest) = take_while1(is_tchar)(input)?;

	// This works because tchar will only ever access ASCII characters which
	// are a subset of UTF-8
	let s = unsafe { AsciiString::from_ascii_unchecked(v) };
	Ok((s, rest))
}


// TODO: 128 to 159 are undefined in ISO-8859-1
// (Obsolete) Text
// `obs-text = %x80-FF`
pub fn is_obs_text(i: u8) -> bool { i >= 0x80 && i <= 0xff }


// `qdtext = HTAB / SP / "!" / %x23-5B ; '#'-'['
//         / %x5D-7E ; ']'-'~'
//         / obs-text`
fn is_qdtext(i: u8) -> bool {
	is_htab(i) || is_sp(i) || (i as char) == '!' ||
	(i >= 0x22 && i <= 0x5b) || (i >= 0x5d && i <= 0x7e) ||
	is_obs_text(i)
}

// `quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )`
parser!(parse_quoted_pair<u8> => seq!(c => {
	c.next(one_of("\\"))?;
	Ok(c.next(like(|i| is_htab(i) || is_sp(i) || is_vchar(i) || is_obs_text(i)))?)
}));

// `quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE`
parser!(pub parse_quoted_string<Latin1String> => seq!(c => {
	c.next(one_of("\""))?;
	let data = c.next(many(alt!(
		like(is_qdtext), parse_quoted_pair)))?;
	c.next(one_of("\""))?;

	Ok(Latin1String::from_bytes(data.into())?)
}));


