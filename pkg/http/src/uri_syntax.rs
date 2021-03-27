// Parsers for the URI syntax.
// This file closely follows RFC 3986.

use crate::uri::*;
use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::*;
use parsing::*;
use std::fmt::Write;
use common::hex;

// TODO: Ensure URLs never get 2K bytes (especially in the incremental form)
// https://stackoverflow.com/questions/417142/what-is-the-maximum-length-of-a-url-in-different-browsers

// TODO: See also https://tools.ietf.org/html/rfc2047

// TODO: Support parsing URIs from human entered text that doesn't contain the scheme (and is assumed to be http)


// RFC 3986: Section 2.1
//
// NOTE: Upper case hex digits should be preferred but either should be accedpted by parsers.
// NOTE: This is strictly ASCII.
// `pct-encoded = "%" HEXDIG HEXDIG`
fn parse_pct_encoded(input: Bytes) -> ParseResult<u8> {
    if input.len() < 3 || input[0] != ('%' as u8) {
        return Err(err_msg("pct-encoded failed"));
    }

    let s = std::str::from_utf8(&input[1..3])?;
    let v = u8::from_str_radix(s, 16)?;

    if v > 0x7f || v <= 0x1f {
        return Err(format_err!(
            "Percent encoded byte outside ASCII range: 0x{:x}",
            v
        ));
    }

    Ok((v, input.slice(3..)))
}

fn serialize_pct_encoded(value: u8, out: &mut Vec<u8>) {
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

// RFC 3986: Section 3
//
// `hier-part = "//" authority path-abempty
// 			  / path-absolute
// 			  / path-rootless
// 			  / path-empty`
parser!(parse_hier_part<(Option<Authority>, UriPath)> => {
    alt!(
        seq!(c => {
            c.next(tag("//"))?;
            let a = c.next(parse_authority)?;
            let p = c.next(parse_path_abempty)?;
            Ok((Some(a), UriPath::AbEmpty(p)))
        }),
        map(parse_path_absolute, |p| (None, UriPath::Absolute(p))),
        map(parse_path_rootless, |p| (None, UriPath::Rootless(p))),
        map(parse_path_empty, |p| (None, UriPath::Empty))
    )
});

/// RFC 3986: Section 3.1
/// 
/// NOTE: This is strictly ASCII.
/// `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
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

// RFC 3986: Section 3.2.1
//
// `userinfo = *( unreserved / pct-encoded / sub-delims / ":" )`
parser!(parse_userinfo<AsciiString> => {
    map(many(alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of(":")
    )), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// RFC 3986: Section 3.2.1
fn serialize_userinfo(info: &AsciiString, out: &mut Vec<u8>) {
    for b in info.as_ref().as_bytes().iter().cloned() {
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
parser!(parse_host<Host> => {
    alt!(
        map(parse_ip_literal, |i| Host::IP(i)),
        map(parse_ipv4_address, |v| Host::IP(IPAddress::V4(v))),
        map(parse_reg_name, |v| Host::Name(v))
    )
});

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
    // TODO: Verify this implementation as it deviates from the 'RFC 3986' definition to allow special cases like '::1'.

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
    padded[(4-i)..].copy_from_slice(&input[0..i]);

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
// NOTE: This is strictly ASCII.
// `reg-name = *( unreserved / pct-encoded / sub-delims )`
parser!(parse_reg_name<AsciiString> => {
    map(many(alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims
    )), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

fn serialize_reg_name(name: &AsciiString, out: &mut String) {
    
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


// RFC 3986: Section 3.3
//
// `path = path-abempty    ; begins with "/" or is empty
// 		 / path-absolute   ; begins with "/" but not "//"
// 		 / path-noscheme   ; begins with a non-colon segment
// 		 / path-rootless   ; begins with a segment
// 		 / path-empty      ; zero characters`
parser!(parse_path<Path> => {
    alt!(
        map(parse_path_abempty, |s| Path::PathAbEmpty(s)),
        map(parse_path_absolute, |s| Path::PathAbsolute(s)),
        map(parse_path_noscheme, |s| Path::PathNoScheme(s)),
        map(parse_path_rootless, |s| Path::PathRootless(s)),
        map(parse_path_empty, |_| Path::PathEmpty)
    )
});

// RFC 3986: Section 3.3
//
// NOTE: This is strictly ASCII.
// `path-abempty = *( "/" segment )`
parser!(parse_path_abempty<Vec<AsciiString>> => {
    many(seq!(c => {
        c.next(one_of("/"))?; // TODO
        c.next(parse_segment)
    }))
});

// RFC 3986: Section 3.3
//
// NOTE: This is strictly ASCII.
// `path-absolute = "/" [ segment-nz *( "/" segment ) ]`
parser!(parse_path_absolute<Vec<AsciiString>> => {
    seq!(c => {
        c.next(one_of("/"))?; // TODO
        c.next(parse_path_rootless)
    })
});

// RFC 3986: Section 3.3
//
// NOTE: This is strictly ASCII.
// `path-noscheme = segment-nz-nc *( "/" segment )`
parser!(parse_path_noscheme<Vec<AsciiString>> => {
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
// NOTE: This is strictly ASCII.
// `path-rootless = segment-nz *( "/" segment )`
parser!(parse_path_rootless<Vec<AsciiString>> => {
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
/// NOTE: This is strictly ASCII.
/// `path-empty = 0<pchar>`
fn parse_path_empty(input: Bytes) -> ParseResult<()> {
    Ok(((), input.clone()))
}

// RFC 3986: Section 3.3
//
// NOTE: This is strictly ASCII.
// `segment = *pchar`
parser!(pub parse_segment<AsciiString> => {
    map(many(parse_pchar),
        |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// RFC 3986: Section 3.3
//
// NOTE: This is strictly ASCII.
// `segment-nz = 1*pchar`
parser!(parse_segment_nz<AsciiString> => {
    map(many1(parse_pchar),
        |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

/// RFC 3986: Section 3.3
///
/// NOTE: This is strictly ASCII.
/// `segment-nz-nc = 1*( unreserved / pct-encoded / sub-delims / "@" )
/// 				; non-zero-length segment without any colon ":"`
fn parse_segment_nz_nc(input: Bytes) -> ParseResult<AsciiString> {
    let p = map(
        many1(alt!(
            parse_unreserved,
            parse_pct_encoded,
            parse_sub_delims,
            one_of("@")
        )),
        |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) },
    );

    p(input)
}

// RFC 3986: Section 3.3
//
// NOTE: This is strictly ASCII.
// `pchar = unreserved / pct-encoded / sub-delims / ":" / "@"`
parser!(parse_pchar<u8> => {
    alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of(":@")
    )
});

// RFC 3986: Section 3.4
//
// NOTE: This is strictly ASCII.
// `query = *( pchar / "/" / "?" )`
parser!(pub parse_query<AsciiString> => parse_fragment);

// RFC 3986: Section 3.5
//
// NOTE: This is strictly ASCII.
// `fragment = *( pchar / "/" / "?" )`
parser!(parse_fragment<AsciiString> => {
    map(many(alt!(
        parse_pchar, one_of("/?")
    )), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

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
            path: path.to_string(),
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
parser!(parse_relative_part<(Option<Authority>, UriPath)> => alt!(
    seq!(c => {
        c.next(tag("//"))?;
        let a = c.next(parse_authority)?;
        let p = c.next(parse_path_abempty)?;
        Ok((Some(a), UriPath::AbEmpty(p)))
    }),
    map(parse_path_absolute, |p| (None, UriPath::Absolute(p))),
    map(parse_path_noscheme, |p| (None, UriPath::Rootless(p))),
    map(parse_path_empty, |p| (None, UriPath::Empty))
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

        Ok(Uri { scheme: Some(s), authority: auth, path: p.to_string(), query: q, fragment: None })
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
            ("ftp://ftp.is.co.za/rfc/rfc1808.txt", Uri {
                scheme: Some(AsciiString::from_str("ftp").unwrap()),
                authority: Some(Authority {
                    user: None,
                    host: Host::Name(AsciiString::from_str("ftp.is.co.za").unwrap()),
                    port: None
                }),
                path: String::from("/rfc/rfc1808.txt"),
                query: None,
                fragment: None
            }),
            ("http://www.ietf.org/rfc/rfc2396.txt", Uri {
                scheme: Some(AsciiString::from_str("http").unwrap()),
                authority: Some(Authority {
                    user: None,
                    host: Host::Name(AsciiString::from_str("www.ietf.org").unwrap()),
                    port: None
                }),
                path: String::from("/rfc/rfc2396.txt"),
                query: None,
                fragment: None
            }),
            ("ldap://[2001:db8::7]/c=GB?objectClass?one", Uri {
                scheme: Some(AsciiString::from_str("ldap").unwrap()),
                authority: Some(Authority {
                    user: None,
                    host: Host::IP(IPAddress::V6(vec![])),
                    port: None
                }),
                path: String::from("/c=GB"),
                query: Some(AsciiString::from_str("objectClass?one").unwrap()),
                fragment: None
            }),
            ("mailto:John.Doe@example.com", Uri {
                scheme: Some(AsciiString::from_str("mailto").unwrap()),
                authority: None,
                path: String::from("John.Doe@example.com"),
                query: None,
                fragment: None
            }),
            ("news:comp.infosystems.www.servers.unix", Uri {
                scheme: Some(AsciiString::from_str("news").unwrap()),
                authority: None,
                path: String::from("comp.infosystems.www.servers.unix"),
                query: None,
                fragment: None
            }),
            ("tel:+1-816-555-1212", Uri {
                scheme: Some(AsciiString::from_str("tel").unwrap()),
                authority: None,
                path: String::from("+1-816-555-1212"),
                query: None,
                fragment: None
            }),
            ("telnet://192.0.2.16:80/", Uri {
                scheme: Some(AsciiString::from_str("telnet").unwrap()),
                authority: Some(Authority {
                    user: None,
                    host: Host::IP(IPAddress::V4(vec![])),
                    port: Some(80)
                }),
                path: String::from("/"),
                query: None,
                fragment: None
            }),
            ("urn:oasis:names:specification:docbook:dtd:xml:4.1.2", Uri {
                scheme: Some(AsciiString::from_str("urn").unwrap()),
                authority: None,
                path: String::from("oasis:names:specification:docbook:dtd:xml:4.1.2"),
                query: None,
                fragment: None
            }),

            // From RFC 3986 Section 3
            ("foo://example.com:8042/over/there?name=ferret#nose", Uri {
                scheme: Some(AsciiString::from_str("foo").unwrap()),
                authority: Some(Authority {
                    user: None,
                    host: Host::Name(AsciiString::from_str("example.com").unwrap()),
                    port: Some(8042)
                }),
                path: String::from("/over/there"),
                query: Some(AsciiString::from_str("name=ferret").unwrap()),
                fragment: Some(AsciiString::from_str("nose").unwrap())
            }),
            ("urn:example:animal:ferret:nose", Uri {
                scheme: Some(AsciiString::from_str("urn").unwrap()),
                authority: None,
                path: String::from("example:animal:ferret:nose"),
                query: None,
                fragment: None
            }),
        ];

        // TODO: Add invalid test cases.

        for (input, output) in test_cases.iter().cloned() {
            assert_eq!(parse_uri(Bytes::from(input)).unwrap(), (output, Bytes::new()));
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
            assert_eq!(parse_ipv4_address(Bytes::from(input.as_bytes())).unwrap(),
                       (output.to_vec(), Bytes::new()));
        }
    }

    #[test]
    fn parse_ipv6_address_test() {
        // TODO:
        // ::ffff:192.0.2.128 is valid
        // ::192.0.2.128 is NOT valid

        let test_cases: &[(&'static str, &[u8])] = &[
            ("::ffff:192.0.2.128", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 0, 2, 128]),
            ("0000:0000:0000:0000:0000:0000:0000:0001", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
            ("::1", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
            ("::", &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            ("2001:0db8:0000:0000:0000:ff00:0042:8329", &[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0xff, 0, 0, 0x42, 0x83, 0x29]),
            ("2001:db8:0:0:0:ff00:42:8329", &[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0xff, 0, 0, 0x42, 0x83, 0x29]),
            ("2001:db8::ff00:42:8329", &[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0xff, 0, 0, 0x42, 0x83, 0x29]),
        ];

        for (input, output) in test_cases {
            assert_eq!(parse_ipv6_address(Bytes::from(input.as_bytes())).unwrap(),
                       (output.to_vec(), Bytes::new()));
        }
    }

    // TODO: Test relative URIs

    // TODO: Also independetly test the IP Address parsers.
}
