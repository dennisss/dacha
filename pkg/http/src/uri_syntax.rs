// Parsers for the URI syntax.
// This file closely follows RFC 3986.

use std::fmt::Write;

use common::bytes::Bytes;
use common::errors::*;
use common::hex;
use net::ip::IPAddress;
use parsing::ascii::*;
use parsing::opaque::OpaqueString;
use parsing::*;

use crate::uri::*;

// TODO: Ensure URLs never get 2K bytes (especially in the incremental form)
// https://stackoverflow.com/questions/417142/what-is-the-maximum-length-of-a-url-in-different-browsers

// TODO: See also https://tools.ietf.org/html/rfc2047

// TODO: Support parsing URIs from human entered text that doesn't contain the
// scheme (and is assumed to be http)

// RFC 3986: Section 2.1
//
//
// NOTE: Upper case hex digits should be preferred but either should be
// accedpted by parsers. `pct-encoded = "%" HEXDIG HEXDIG`
fn parse_pct_encoded(input: Bytes) -> ParseResult<u8> {
    if input.len() < 3 || input[0] != ('%' as u8) {
        return Err(err_msg("pct-encoded failed"));
    }

    let s = std::str::from_utf8(&input[1..3])?;
    let v = u8::from_str_radix(s, 16)?;
    Ok((v, input.slice(3..)))
}

fn serialize_pct_encoded(value: u8, out: &mut Vec<u8>) {
    // NOTE: For standardization, there is a preference specified in the RFC to use
    // upper case hex characters.
    // TODO: Consider checking if the given value is in the ascii range.
    out.extend_from_slice(format!("%{:02X}", value).as_bytes());
}

/// RFC 3986: Section 2.2
///
/// NOTE: This is strictly ASCII.
/// NOTE: These must be 'pct-encoded' when appearing in a segment.
/// `reserved = gen-delims / sub-delims`
fn is_reserved(i: u8) -> bool {
    is_gen_delims(i) || is_sub_delims(i)
}

/// RFC 3986: Section 2.2
/// NOTE: This is strictly ASCII.
/// `gen-delims = ":" / "/" / "?" / "#" / "[" / "]" / "@"`
fn is_gen_delims(i: u8) -> bool {
    i.is_one_of(b":/?#[]@")
}

/// RFC 3986: Section 2.2
///
/// NOTE: This is strictly ASCII.
/// `"!" / "$" / "&" / "'" / "(" / ")" / "*" / "+" / "," / ";" / "="`
fn is_sub_delims(i: u8) -> bool {
    i.is_one_of(b"!$&'()*+,;=")
}

// RFC 3986: Section 2.2
//
// NOTE: This is strictly ASCII.
parser!(parse_sub_delims<u8> => like(is_sub_delims));

// RFC 3986: Section 2.3
//
// NOTE: This is strictly ASCII.
// `unreserved = ALPHA / DIGIT / "-" / "." / "_" / "~"`
parser!(parse_unreserved<u8> => {
    like(|i: u8| {
        is_unreserved(i)
    })
});

// RFC 3986: Section 2.3
fn is_unreserved(i: u8) -> bool {
    // TODO: What happens if the byte is out of the ASCII range?
    let c = i as char;
    c.is_ascii_alphanumeric() || c.is_one_of("-._~")
}

// RFC 3986: Section 3
//
// `URI = scheme ":" hier-part [ "?" query ] [ "#" fragment ]`
parser!(pub parse_uri<Uri> => {
    seq!(c => {
        let mut u = c.next(parse_absolute_uri)?;
        u.fragment = c.next(opt(seq!(c => {
            c.next(one_of("#"))?;
            c.next(parse_fragment)
        })))?;

        Ok(u)
    })
});

/// The algorithm for this is defined in:
/// RFC 3986: Secion 5.3
///
/// TODO: Improve this.
pub fn serialize_uri(uri: &Uri, out: &mut Vec<u8>) -> Result<()> {
    if let Some(scheme) = &uri.scheme {
        serialize_scheme(scheme, out)?;
        out.push(b':');
    }

    if let Some(authority) = &uri.authority {
        out.extend_from_slice(b"//");
        serialize_authority(authority, out)?;
    }

    {
        // TODO: Key things we need to validate:
        // 1. There are no '?' symbols in this string.
        // 2. If there are any percent encoded components, they are valid.
        // 3. Can't start with '//'
        // 4. Doesn't contain any non-ascii characters (should have all been encoded)
        //
        // NOTE: Because we can't distinguish between '/' and the pct-encoded version of
        // it in this stage, we ideally shouldn't try to decode it yet.
        out.extend_from_slice(uri.path.as_ref().as_bytes());

        // for (i, segment) in uri.path.segments().iter().enumerate() {
        //     if i != 0 || uri.path.is_absolute() {
        //         out.push(b'/');
        //     }

        //     out.extend_from_slice(segment.as_bytes());
        // }
    }

    // TODO: Definately need to improve these by a lot.
    // TOOD: We shouldn't be doing any serialization of these (only sanitization).
    if let Some(query) = &uri.query {
        out.push(b'?');

        // TODO: Instead, we can try automatically encoding anything that violated the
        // syntax?
        for byte in query.as_ref().as_bytes() {
            if *byte == b'#' {
                return Err(err_msg("Invalid query"));
            }
        }

        out.extend_from_slice(query.as_ref().as_bytes());
    }

    if let Some(fragment) = &uri.fragment {
        out.push(b'#');
        out.extend_from_slice(fragment.as_ref().as_bytes());
        // serialize_fragment(fragment, out);
    }

    Ok(())
}

// RFC 3986: Section 3
//
// `hier-part = "//" authority path-abempty
// 			  / path-absolute
// 			  / path-rootless
// 			  / path-empty`
parser!(parse_hier_part<(Option<Authority>, AsciiString)> => {
    alt!(
        seq!(c => {
            c.next(tag("//"))?;
            let a = c.next(parse_authority)?;
            let p = c.next(slice(parse_path_abempty))?;
            Ok((Some(a), AsciiString::from(p).unwrap()))
        }),
        map(slice(alt!(
            map(parse_path_absolute, |_| ()), map(parse_path_rootless, |_| ()), parse_path_empty
        )), |data| (None, AsciiString::from(data).unwrap()))
    )
});

/// RFC 3986: Section 3.1
///
/// NOTE: This is strictly ASCII.
/// `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
///
/// TODO: Implement as a regular expression.
fn parse_scheme(input: Bytes) -> ParseResult<AsciiString> {
    let mut i = 0;
    while i < input.len() {
        let c = input[i];
        let valid = if i == 0 {
            (c as char).is_ascii_alphabetic()
        } else {
            (c as char).is_ascii_alphanumeric() || c.is_one_of(b"+-.")
        };

        if !valid {
            break;
        }

        i += 1;
    }

    if i < 1 {
        Err(err_msg("Failed to parse URI scheme."))
    } else {
        let mut v = input.clone();
        let rest = v.split_off(i);
        let s = unsafe { AsciiString::from_ascii_unchecked(v) };
        Ok((s, rest))
    }
}

fn serialize_scheme(value: &AsciiString, out: &mut Vec<u8>) -> Result<()> {
    // Validate syntax
    complete(parse_scheme)(value.data.clone())?;

    out.reserve(value.data.len());
    out.extend_from_slice(value.as_ref().as_bytes());
    Ok(())
}

// RFC 3986: Section 3.2
//
// `authority = [ userinfo "@" ] host [ ":" port ]`
parser!(pub parse_authority<Authority> => {
    seq!(c => {
        let user = c.next(opt(seq!(c => {
            let u = c.next(parse_userinfo)?;
            c.next(one_of("@"))?;
            Ok(u)
        })))?;

        let h = c.next(parse_host)?;

        let p_raw = c.next(opt(seq!(c => {
            c.next(one_of(":"))?;
            c.next(parse_port)
        })))?;

        // Unwrap Option<Option<usize>> as just having ':' without any port after it is still valid syntax.
        let p = if let Some(Some(p)) = p_raw { Some(p) } else { None };

        Ok(Authority { user, host: h, port: p })
    })
});

pub fn serialize_authority(value: &Authority, out: &mut Vec<u8>) -> Result<()> {
    if let Some(user) = &value.user {
        serialize_userinfo(user, out);
        out.push(b'@');
    }

    serialize_host(&value.host, out)?;

    if let Some(port) = &value.port {
        let s = format!(":{}", port);
        out.extend_from_slice(s.as_bytes());
    }

    Ok(())
}

// RFC 3986: Section 3.2.1
//
// `userinfo = *( unreserved / pct-encoded / sub-delims / ":" )`
parser!(parse_userinfo<OpaqueString> => {
    map(many(alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of(":")
    )), |s| OpaqueString::from(s))
});

// RFC 3986: Section 3.2.1
fn serialize_userinfo(info: &OpaqueString, out: &mut Vec<u8>) {
    for b in info.as_bytes().iter().cloned() {
        if is_unreserved(b) || is_sub_delims(b) || b == (':' as u8) {
            out.push(b);
        } else {
            serialize_pct_encoded(b, out);
        }
    }
}

// RFC 3986: Section 3.2.2
//
// `host = IP-literal / IPv4address / reg-name`
parser!(pub(crate) parse_host<Host> => {
    alt!(
        map(parse_ip_literal, |i| Host::IP(i)),
        map(parse_ipv4_address, |v| Host::IP(IPAddress::V4(v))),
        map(parse_reg_name, |v| Host::Name(v))
    )
});

fn serialize_host(host: &Host, out: &mut Vec<u8>) -> Result<()> {
    match host {
        Host::IP(ip) => serialize_ip(ip, out)?,
        Host::Name(name) => serialize_reg_name(name, out),
    }

    Ok(())
}

// RFC 3986: Section 3.2.2
//
// TODO: Add IPv6addrz as in https://tools.ietf.org/html/rfc6874
// `IP-literal = "[" ( IPv6address / IPvFuture  ) "]"`
parser!(parse_ip_literal<IPAddress> => {
    seq!(c => {
        c.next(one_of("["))?;
        let addr = c.next(alt!(
            map(parse_ipv6_address, |v| IPAddress::V6(v)),
            map(parse_ip_vfuture, |v| IPAddress::VFuture(v))
        ))?;
        c.next(one_of("]"))?;
        Ok(addr)
    })
});

pub fn serialize_ip(value: &IPAddress, out: &mut Vec<u8>) -> Result<()> {
    let s: String = match value {
        IPAddress::V4(v) => {
            if v.len() != 4 {
                return Err(err_msg("IPv4 must be 4 bytes long"));
            }

            format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3])
        }
        IPAddress::V6(v) => {
            if v.len() != 16 {
                return Err(err_msg("IPv6 must be 16 bytes long"));
            }

            /// (start_index, end_index) of the longest range of zero bytes seen
            /// in the address. Initialized to a range that is
            /// trivially outside the address length.
            let mut longest_zero_range = (255, 255);
            {
                let mut cur_zero_range = (0, 0);
                for i in 0..v.len() {
                    if v[i] == 0 {
                        if cur_zero_range.1 == i {
                            cur_zero_range = (cur_zero_range.0, i + 1);
                        } else {
                            cur_zero_range = (i, i + 1);
                        }

                        if cur_zero_range.1 - cur_zero_range.0
                            > longest_zero_range.1 - longest_zero_range.0
                        {
                            longest_zero_range = cur_zero_range;
                        }
                    }
                }
            }

            let mut s = String::new();

            for (i, byte) in v.iter().enumerate() {
                if i >= longest_zero_range.0 && i < longest_zero_range.1 {
                    continue;
                }

                write!(&mut s, "{:02x}", byte);
                if i < v.len() {
                    s.push(':');
                }
                if i + 1 == longest_zero_range.0 {
                    s.push(':');
                }
            }

            s
        }
        IPAddress::VFuture(v) => {
            return Err(err_msg("Serializing future IP formats not yet supported"));
        }
    };

    out.extend_from_slice(s.as_bytes());
    Ok(())
}

// RFC 3986: Section 3.2.2
//
// `IPvFuture = "v" 1*HEXDIG "." 1*( unreserved / sub-delims / ":" )`
parser!(parse_ip_vfuture<Vec<u8>> => {
    seq!(c => {
        let mut out = vec![];
        out.push(c.next(one_of(b"v"))?);
        out.push(c.next(like(|i| (i as char).is_digit(16)))?);
        out.push(c.next(one_of(b"."))?);

        let rest = c.next(many1(alt!(
            parse_unreserved, parse_sub_delims, one_of(":")
        )))?;
        out.extend_from_slice(&rest);
        Ok(out)
    })
});

/// RFC 3986: Section 3.2.2
///
/// `IPv6address =                            6( h16 ":" ) ls32
/// 				/                       "::" 5( h16 ":" ) ls32
/// 				/ [               h16 ] "::" 4( h16 ":" ) ls32
/// 				/ [ *1( h16 ":" ) h16 ] "::" 3( h16 ":" ) ls32
/// 				/ [ *2( h16 ":" ) h16 ] "::" 2( h16 ":" ) ls32
/// 				/ [ *3( h16 ":" ) h16 ] "::"    h16 ":"   ls32
/// 				/ [ *4( h16 ":" ) h16 ] "::"              ls32
/// 				/ [ *5( h16 ":" ) h16 ] "::"              h16
/// 				/ [ *6( h16 ":" ) h16 ] "::"`
fn parse_ipv6_address(input: Bytes) -> ParseResult<Vec<u8>> {
    // TODO: Verify this implementation as it deviates from the 'RFC 3986'
    // definition to allow special cases like '::1'.

    // Parses `h16 ":"`
    let h16_colon = seq!(c => {
        let v = c.next(parse_h16)?;
        c.next(one_of(":"))?;
        Ok(v)
    });

    // Parses `N (h16 ":")`.
    // Up to 'max_bytes' (which must be divisible by 2)
    let many_h16 = |max_bytes: usize| {
        seq!(c => {
            let mut out = vec![];
            while out.len() < max_bytes {
                if let Some(h16) = c.next(opt(h16_colon))? {
                    out.extend_from_slice(&h16);
                } else {
                    break;
                }
            }

            Ok(out)
        })
    };

    let p = seq!(c => {
        // Parse first half of the address
        let mut out = c.next(many_h16(14))?;

        let mut rest = vec![];

        let padded = c.next(opt(tag(if out.len() == 0 { "::" } else { ":" })))?.is_some();
        if padded {
            rest = c.next(many_h16(14 - out.len()))?;
        }

        if let Some(ipv4) = c.next(opt(parse_ipv4_address))? {
            rest.extend_from_slice(&ipv4);
        } else if let Some(h16) = c.next(opt(parse_h16))? {
            rest.extend_from_slice(&h16);
        }

        if padded {
            for i in 0..(16 - (out.len() + rest.len())) {
                out.push(0);
            }
        }

        out.extend_from_slice(&rest);

        if out.len() != 16 {
            return Err(err_msg("Too few bytes in IPv6 address"));
        }

        Ok(out)
    });

    p(input)
}

/// RFC 3986: Section 3.2.2
///
/// `h16 = 1*4HEXDIG`
fn parse_h16(input: Bytes) -> ParseResult<Vec<u8>> {
    let mut i = 0;
    while i < 4 && i < input.len() {
        if input[i].is_ascii_hexdigit() {
            i += 1;
        } else {
            break;
        }
    }

    if i < 1 {
        return Err(err_msg("h16: input too short"));
    }

    let mut padded: [u8; 4] = *b"0000";
    padded[(4 - i)..].copy_from_slice(&input[0..i]);

    let mut decoded = hex::decode(padded).unwrap();
    if decoded.len() == 1 {
        decoded.insert(0, 0);
    }

    assert_eq!(decoded.len(), 2);

    Ok((decoded, input.slice(i..)))
}

/// RFC 3986: Section 3.2.2
///
/// `ls32 = ( h16 ":" h16 ) / IPv4address`
fn parse_ls32(input: Bytes) -> ParseResult<Vec<u8>> {
    let p = alt!(
        seq!(c => {
            let mut bytes = vec![];
            bytes.extend_from_slice(&c.next(parse_h16)?);
            c.next(one_of(":"))?;
            bytes.extend_from_slice(&c.next(parse_h16)?);
            Ok(bytes)
        }),
        parse_ipv4_address
    );

    p(input)
}

/// RFC 3986: Section 3.2.2
///
/// `IPv4address = dec-octet "." dec-octet "." dec-octet "." dec-octet`
fn parse_ipv4_address(input: Bytes) -> ParseResult<Vec<u8>> {
    let p = seq!(c => {
        let a1 = c.next(parse_dec_octet)?;
        c.next(one_of("."))?;
        let a2 = c.next(parse_dec_octet)?;
        c.next(one_of("."))?;
        let a3 = c.next(parse_dec_octet)?;
        c.next(one_of("."))?;
        let a4 = c.next(parse_dec_octet)?;
        Ok(vec![a1, a2, a3, a4])
    });

    p(input)
}

/// RFC 3986: Section 3.2.2
/// `dec-octet = DIGIT                 ; 0-9
/// 			  / %x31-39 DIGIT         ; 10-99
/// 			  / "1" 2DIGIT            ; 100-199
/// 			  / "2" %x30-34 DIGIT     ; 200-249
/// 			  / "25" %x30-35          ; 250-255`
fn parse_dec_octet(input: Bytes) -> ParseResult<u8> {
    // TODO: Validate only taking 3 characters.
    let (u, rest) = take_while1(|i: u8| (i as char).is_digit(10))(input)?;
    let s = std::str::from_utf8(&u)?;
    let v = u8::from_str_radix(s, 10)?;
    Ok((v, rest))
}

// RFC 3986: Section 3.2.2
//
// According to the RFC, when percent encoding is used in a name, it should only
// be for representing UTF-8 octets.
//
// `reg-name = *( unreserved / pct-encoded / sub-delims )`
parser!(parse_reg_name<String> => {
    and_then(many(alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims
    )), |s: Vec<u8>| Ok(String::from_utf8(s)?) )
});

fn serialize_reg_name(name: &str, out: &mut Vec<u8>) {
    out.reserve(name.len());
    for byte in name.as_bytes().iter().cloned() {
        if is_unreserved(byte) || is_sub_delims(byte) {
            out.push(byte)
        } else {
            serialize_pct_encoded(byte, out);
        }
    }
}

/// RFC 3986: Section 3.2.3
///
/// `port = *DIGIT`
fn parse_port(input: Bytes) -> ParseResult<Option<usize>> {
    let (v, rest) = take_while1(|i| (i as char).is_digit(10))(input)?;
    if v.len() == 0 {
        return Ok((None, rest));
    }
    let s = std::str::from_utf8(&v)?;
    let p = usize::from_str_radix(s, 10)?;
    Ok((Some(p), rest))
}

// TODO: Is this ever used?
// RFC 3986: Section 3.3
//
// `path = path-abempty    ; begins with "/" or is empty
// 		 / path-absolute   ; begins with "/" but not "//"
// 		 / path-noscheme   ; begins with a non-colon segment
// 		 / path-rootless   ; begins with a segment
// 		 / path-empty      ; zero characters`
parser!(parse_path<RawPath> => {
    alt!(
        map(parse_path_abempty, |s| RawPath::PathAbEmpty(s)),
        map(parse_path_absolute, |s| RawPath::PathAbsolute(s)),
        map(parse_path_noscheme, |s| RawPath::PathNoScheme(s)),
        map(parse_path_rootless, |s| RawPath::PathRootless(s)),
        map(parse_path_empty, |_| RawPath::PathEmpty)
    )
});

// RFC 3986: Section 3.3
//
// `path-abempty = *( "/" segment )`
parser!(parse_path_abempty<Vec<OpaqueString>> => {
    many(seq!(c => {
        c.next(one_of("/"))?; // TODO
        c.next(parse_segment)
    }))
});

// RFC 3986: Section 3.3
//
// `path-absolute = "/" [ segment-nz *( "/" segment ) ]`
parser!(parse_path_absolute<Vec<OpaqueString>> => {
    seq!(c => {
        c.next(one_of("/"))?; // TODO
        c.next(parse_path_rootless)
    })
});

// RFC 3986: Section 3.3
//
// `path-noscheme = segment-nz-nc *( "/" segment )`
parser!(parse_path_noscheme<Vec<OpaqueString>> => {
    seq!(c => {
        let first_seg = c.next(parse_segment_nz_nc)?;
        let next_segs = c.next(many(seq!(c => {
            c.next(one_of("/"))?;
            c.next(parse_segment)
        })))?;

        let mut segs = vec![];
        segs.push(first_seg);
        segs.extend(next_segs.into_iter());
        Ok(segs)
    })
});

// RFC 3986: Section 3.3
//
// `path-rootless = segment-nz *( "/" segment )`
parser!(parse_path_rootless<Vec<OpaqueString>> => {
    seq!(c => {
        let first_seg = c.next(parse_segment_nz)?;
        let next_segs = c.next(many(seq!(c => {
            c.next(one_of("/"))?;
            c.next(parse_segment)
        })))?;

        let mut segs = vec![];
        segs.push(first_seg);
        segs.extend(next_segs.into_iter());
        Ok(segs)
    })
});

/// RFC 3986: Section 3.3
///
/// `path-empty = 0<pchar>`
fn parse_path_empty(input: Bytes) -> ParseResult<()> {
    Ok(((), input.clone()))
}

// RFC 3986: Section 3.3
//
// `segment = *pchar`
parser!(pub parse_segment<OpaqueString> => {
    map(many(parse_pchar), |s| OpaqueString::from(s))
});

// RFC 3986: Section 3.3
//
// `segment-nz = 1*pchar`
parser!(parse_segment_nz<OpaqueString> => {
    map(many1(parse_pchar), |s| OpaqueString::from(s))
});

/// RFC 3986: Section 3.3
///
/// `segment-nz-nc = 1*( unreserved / pct-encoded / sub-delims / "@" )
/// 				; non-zero-length segment without any colon ":"`
fn parse_segment_nz_nc(input: Bytes) -> ParseResult<OpaqueString> {
    let p = map(
        many1(alt!(
            parse_unreserved,
            parse_pct_encoded,
            parse_sub_delims,
            one_of("@")
        )),
        |s| OpaqueString::from(s),
    );

    p(input)
}

// RFC 3986: Section 3.3
//
// `pchar = unreserved / pct-encoded / sub-delims / ":" / "@"`
//
// TODO: Parse as a regular expression
parser!(parse_pchar<u8> => {
    alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of(":@")
    )
});

fn serialize_pchar(v: u8, out: &mut Vec<u8>) {
    if is_unreserved(v) || is_sub_delims(v) || v == b':' || v == b'@' {
        out.push(v);
    } else {
        serialize_pct_encoded(v, out);
    }
}

// RFC 3986: Section 3.4
//
// `query = *( pchar / "/" / "?" )`
parser!(pub parse_query<AsciiString> => parse_fragment);

/*
pub fn serialize_query(value: &OpaqueString, out: &mut Vec<u8>) {
    serialize_fragment(value, out);
}
*/

// RFC 3986: Section 3.5
//
// `fragment = *( pchar / "/" / "?" )`
parser!(parse_fragment<AsciiString> => {
    map(slice(many(alt!(
        parse_pchar, one_of("/?")
    ))), |s| AsciiString::from(s).unwrap())
});

/*
pub fn serialize_fragment(value: &OpaqueString, out: &mut Vec<u8>) {
    for char in value.as_bytes().iter().cloned() {
        if char == b'/' || char == b'?' {
            out.push(char);
        } else {
            serialize_pchar(char, out);
        }
    }
}
*/

// RFC 3986: Section 4.1
//
// `URI-reference = URI / relative-ref`
parser!(parse_uri_reference<Uri> => {
    alt!(
        parse_uri,
        parse_relative_ref
    )
});

// RFC 3986: Section 4.2
//
// `relative-ref = relative-part [ "?" query ] [ "#" fragment ]`
parser!(parse_relative_ref<Uri> => {
    seq!(c => {
        let (authority, path) = c.next(parse_relative_part)?;

        let query = c.next(opt(seq!(c => {
            c.next(one_of("?"))?;
            c.next(parse_query)
        })))?;

        let fragment = c.next(opt(seq!(c => {
            c.next(one_of("#"))?;
            c.next(parse_fragment)
        })))?;

        Ok(Uri {
            scheme: None,
            authority,
            path,
            query,
            fragment
        })
    })
});

// RFC 3986: Section 4.2
//
// `relative-part = "//" authority path-abempty
// 				  / path-absolute
// 				  / path-noscheme
// 				  / path-empty`
parser!(parse_relative_part<(Option<Authority>, AsciiString)> => alt!(
    seq!(c => {
        c.next(tag("//"))?;
        let a = c.next(parse_authority)?;
        let p = c.next(slice(parse_path_abempty))?;
        Ok((Some(a), AsciiString::from(p).unwrap()))
    }),
    map(slice(alt!(
        map(parse_path_absolute, |_| ()), map(parse_path_noscheme, |_| ()), parse_path_empty
    )), |data: Bytes| (None, AsciiString::from(data).unwrap()))
));

// RFC 3986: Section 4.3
//
// `absolute-URI = scheme ":" hier-part [ "?" query ]`
parser!(pub parse_absolute_uri<Uri> => {
    seq!(c => {
        let s = c.next(parse_scheme)?;
        c.next(one_of(":"))?;
        let (auth, p) = c.next(parse_hier_part)?;
        let q = c.next(opt(seq!(c => {
            c.next(one_of("?"))?;
            c.next(parse_query)
        })))?;

        Ok(Uri { scheme: Some(s), authority: auth, path: p, query: q, fragment: None })
    })
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_uri_test() {
        // TODO: Testing examples from here:
        // https://en.wikipedia.org/wiki/Uniform_Resource_Identifier#Examples

        let (v, rest) = parse_uri("http://a@google.com/gfgf/ff".into()).unwrap();
        assert_eq!(rest.as_ref(), &[]);
        println!("{:?}", v);
    }

    #[test]
    fn parse_uri2_test() {
        let test_cases: &[(&'static str, Uri)] = &[
            // Valid URIs based on 'RFC 3986 1.1.2'
            (
                "ftp://ftp.is.co.za/rfc/rfc1808.txt",
                Uri {
                    scheme: Some(AsciiString::from("ftp").unwrap()),
                    authority: Some(Authority {
                        user: None,
                        host: Host::Name("ftp.is.co.za".to_string()),
                        port: None,
                    }),
                    path: AsciiString::from("/rfc/rfc1808.txt").unwrap(),
                    // path: UriPath::new(true, &["rfc", "rfc1808.txt"]),
                    query: None,
                    fragment: None,
                },
            ),
            (
                "http://www.ietf.org/rfc/rfc2396.txt",
                Uri {
                    scheme: Some(AsciiString::from("http").unwrap()),
                    authority: Some(Authority {
                        user: None,
                        host: Host::Name("www.ietf.org".to_string()),
                        port: None,
                    }),
                    path: AsciiString::from("/rfc/rfc2396.txt").unwrap(),
                    // path: UriPath::new(true, &["rfc", "rfc2396.txt"]),
                    query: None,
                    fragment: None,
                },
            ),
            (
                "ldap://[2001:db8::7]/c=GB?objectClass?one",
                Uri {
                    scheme: Some(AsciiString::from("ldap").unwrap()),
                    authority: Some(Authority {
                        user: None,
                        host: Host::IP(IPAddress::V6(vec![
                            0x20, 0x01, 0x0D, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x07,
                        ])),
                        port: None,
                    }),
                    path: AsciiString::from("/c=GB").unwrap(),
                    // path: UriPath::new(true, &["c=GB"]),
                    query: Some(AsciiString::from("objectClass?one").unwrap()),
                    fragment: None,
                },
            ),
            (
                "mailto:John.Doe@example.com",
                Uri {
                    scheme: Some(AsciiString::from("mailto").unwrap()),
                    authority: None,
                    path: AsciiString::from("John.Doe@example.com").unwrap(),
                    // path: UriPath::new(false, &["John.Doe@example.com"]),
                    query: None,
                    fragment: None,
                },
            ),
            (
                "news:comp.infosystems.www.servers.unix",
                Uri {
                    scheme: Some(AsciiString::from("news").unwrap()),
                    authority: None,
                    path: AsciiString::from("comp.infosystems.www.servers.unix").unwrap(),
                    // path: UriPath::new(false, &["comp.infosystems.www.servers.unix"]),
                    query: None,
                    fragment: None,
                },
            ),
            (
                "tel:+1-816-555-1212",
                Uri {
                    scheme: Some(AsciiString::from("tel").unwrap()),
                    authority: None,
                    path: AsciiString::from("+1-816-555-1212").unwrap(),
                    // path: UriPath::new(false, &["+1-816-555-1212"]),
                    query: None,
                    fragment: None,
                },
            ),
            (
                "telnet://192.0.2.16:80/",
                Uri {
                    scheme: Some(AsciiString::from("telnet").unwrap()),
                    authority: Some(Authority {
                        user: None,
                        host: Host::IP(IPAddress::V4(vec![192, 0, 2, 16])),
                        port: Some(80),
                    }),
                    path: AsciiString::from("/").unwrap(),
                    // path: UriPath::new(true, &[""]),
                    query: None,
                    fragment: None,
                },
            ),
            (
                "urn:oasis:names:specification:docbook:dtd:xml:4.1.2",
                Uri {
                    scheme: Some(AsciiString::from("urn").unwrap()),
                    authority: None,
                    path: AsciiString::from("oasis:names:specification:docbook:dtd:xml:4.1.2")
                        .unwrap(),
                    // path: UriPath::new(false,
                    // &["oasis:names:specification:docbook:dtd:xml:4.1.2"]),
                    query: None,
                    fragment: None,
                },
            ),
            // From RFC 3986 Section 3
            (
                "foo://example.com:8042/over/there?name=ferret#nose",
                Uri {
                    scheme: Some(AsciiString::from("foo").unwrap()),
                    authority: Some(Authority {
                        user: None,
                        host: Host::Name("example.com".to_string()),
                        port: Some(8042),
                    }),
                    path: AsciiString::from("/over/there").unwrap(),
                    // path: UriPath::new(true, &["over", "there"]),
                    query: Some(AsciiString::from("name=ferret").unwrap()),
                    fragment: Some(AsciiString::from("nose").unwrap()),
                },
            ),
            (
                "urn:example:animal:ferret:nose",
                Uri {
                    scheme: Some(AsciiString::from("urn").unwrap()),
                    authority: None,
                    path: AsciiString::from("example:animal:ferret:nose").unwrap(),
                    // path: UriPath::new(false, &["example:animal:ferret:nose"]),
                    query: None,
                    fragment: None,
                },
            ),
            (
                "urn:/example/world",
                Uri {
                    scheme: Some(AsciiString::from("urn").unwrap()),
                    authority: None,
                    path: AsciiString::from("/example/world").unwrap(), /* UriPath::new(true,
                                                                         * &["example",
                                                                         * "world"]), */
                    query: None,
                    fragment: None,
                },
            ),
            (
                "https://localhost:8000",
                Uri {
                    scheme: Some(AsciiString::from("https").unwrap()),
                    authority: Some(Authority {
                        user: None,
                        host: Host::Name("localhost".to_string()),
                        port: Some(8000),
                    }),
                    // TODO: Normalize this to '/' as it is actually absolute.
                    path: AsciiString::from("").unwrap(),
                    // path: UriPath::new(true, &[]),
                    query: None,
                    fragment: None,
                },
            ),
        ];

        // TODO: Add invalid test cases.

        for (input, output) in test_cases.iter().cloned() {
            assert_eq!(
                parse_uri(Bytes::from(input)).unwrap(),
                (output, Bytes::new())
            );
        }
    }

    #[test]
    fn parse_ipv4_address_test() {
        let test_cases: &[(&'static str, &[u8])] = &[
            ("192.168.0.1", &[192, 168, 0, 1]),
            ("255.255.255.255", &[255, 255, 255, 255]),
            ("10.0.0.1", &[10, 0, 0, 1]),
        ];

        for (input, output) in test_cases {
            assert_eq!(
                parse_ipv4_address(Bytes::from(input.as_bytes())).unwrap(),
                (output.to_vec(), Bytes::new())
            );
        }
    }

    #[test]
    fn parse_ipv6_address_test() {
        // TODO:
        // ::ffff:192.0.2.128 is valid
        // ::192.0.2.128 is NOT valid

        let test_cases: &[(&'static str, &[u8])] = &[
            (
                "::ffff:192.0.2.128",
                &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 0, 2, 128],
            ),
            (
                "0000:0000:0000:0000:0000:0000:0000:0001",
                &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
            ),
            ("::1", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
            ("::", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            (
                "2001:0db8:0000:0000:0000:ff00:0042:8329",
                &[
                    0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0xff, 0, 0, 0x42, 0x83, 0x29,
                ],
            ),
            (
                "2001:db8:0:0:0:ff00:42:8329",
                &[
                    0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0xff, 0, 0, 0x42, 0x83, 0x29,
                ],
            ),
            (
                "2001:db8::ff00:42:8329",
                &[
                    0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0xff, 0, 0, 0x42, 0x83, 0x29,
                ],
            ),
        ];

        for (input, output) in test_cases {
            assert_eq!(
                parse_ipv6_address(Bytes::from(input.as_bytes())).unwrap(),
                (output.to_vec(), Bytes::new())
            );
        }
    }

    // TODO: Test relative URIs

    // TODO: Also independetly test the IP Address parsers.
}
