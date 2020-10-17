use crate::uri::*;
use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::*;
use parsing::*;

// TODO: Ensure URLs never get 2K bytes (especially in the incremental form)
// https://stackoverflow.com/questions/417142/what-is-the-maximum-length-of-a-url-in-different-browsers

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

// `URI-reference = URI / relative-ref`

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

// `relative-ref = relative-part [ "?" query ] [ "#" fragment ]`

// `relative-part = "//" authority path-abempty
// 				  / path-absolute
// 				  / path-noscheme
// 				  / path-empty`

// NOTE: This is strictly ASCII.
// `scheme = ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
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
        Err(err_msg("Failed to parse scheme"))
    } else {
        let mut v = input.clone();
        let rest = v.split_off(i);
        let s = unsafe { AsciiString::from_ascii_unchecked(v) };
        Ok((s, rest))
    }
}

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

// `userinfo = *( unreserved / pct-encoded / sub-delims / ":" )`
parser!(parse_userinfo<AsciiString> => {
    map(many(alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of(":")
    )), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// `host = IP-literal / IPv4address / reg-name`
parser!(parse_host<Host> => {
    alt!(
        map(parse_ip_literal, |i| Host::IP(i)),
        map(parse_ipv4_address, |v| Host::IP(IPAddress::V4(v))),
        map(parse_reg_name, |v| Host::Name(v))
    )
});

// `port = *DIGIT`
fn parse_port(input: Bytes) -> ParseResult<Option<usize>> {
    let (v, rest) = take_while1(|i| (i as char).is_digit(10))(input)?;
    if v.len() == 0 {
        return Ok((None, rest));
    }
    let s = std::str::from_utf8(&v)?;
    let p = usize::from_str_radix(s, 10)?;
    Ok((Some(p), rest))
}

// TODO: See also https://tools.ietf.org/html/rfc2047

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

// `IPv6address =                            6( h16 ":" ) ls32
// 				/                       "::" 5( h16 ":" ) ls32
// 				/ [               h16 ] "::" 4( h16 ":" ) ls32
// 				/ [ *1( h16 ":" ) h16 ] "::" 3( h16 ":" ) ls32
// 				/ [ *2( h16 ":" ) h16 ] "::" 2( h16 ":" ) ls32
// 				/ [ *3( h16 ":" ) h16 ] "::"    h16 ":"   ls32
// 				/ [ *4( h16 ":" ) h16 ] "::"              ls32
// 				/ [ *5( h16 ":" ) h16 ] "::"              h16
// 				/ [ *6( h16 ":" ) h16 ] "::"`
fn parse_ipv6_address(input: Bytes) -> ParseResult<Vec<u8>> {
    let many_h16 = |n: usize| {
        seq!(c => {
            let mut out = vec![];
            for i in 0..n {
                out.extend(c.next(parse_h16)?.into_iter());
                c.next(one_of(":"))?;
            }
            Ok(out)
        })
    };

    let p = alt!(
        seq!(c => {
            let mut out = c.next(many_h16(6))?;
            out.extend(c.next(parse_ls32)?.into_iter());
            Ok(out)
        }),
        seq!(c => {
            c.next(tag("::"))?;
            let mut out = c.next(many_h16(6))?;
            out.extend(c.next(parse_ls32)?.into_iter());
            Ok(out)
        }) /* TODO: Need to implement all cases and fill in missing bytes */

           /* seq!(c => {
            * 	let out = c.next(opt(h16))
            * }) */
    );

    p(input)
}

// `h16 = 1*4HEXDIG`
fn parse_h16(input: Bytes) -> ParseResult<Vec<u8>> {
    if input.len() < 4 {
        return Err(err_msg("h16: input too short"));
    }
    for i in 0..4 {
        if !(input[i] as char).is_digit(16) {
            return Err(err_msg("h16 not digit"));
        }
    }

    Ok((Vec::from(&input[0..4]), input.slice(4..)))
}

// `ls32 = ( h16 ":" h16 ) / IPv4address`
fn parse_ls32(input: Bytes) -> ParseResult<Vec<u8>> {
    let p = alt!(
        seq!(c => {
            let mut bytes = vec![];
            bytes.extend(c.next(parse_h16)?.into_iter());
            c.next(one_of(":"))?;
            bytes.extend(c.next(parse_h16)?.into_iter());
            Ok(bytes)
        }),
        parse_ipv4_address
    );

    p(input)
}

// `IPv4address = dec-octet "." dec-octet "." dec-octet "." dec-octet`
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

// `dec-octet = DIGIT                 ; 0-9
// 			  / %x31-39 DIGIT         ; 10-99
// 			  / "1" 2DIGIT            ; 100-199
// 			  / "2" %x30-34 DIGIT     ; 200-249
// 			  / "25" %x30-35          ; 250-255`
fn parse_dec_octet(input: Bytes) -> ParseResult<u8> {
    // TODO: Validate only taking 3 characters.
    let (u, rest) = take_while1(|i: u8| (i as char).is_digit(10))(input)?;
    let s = std::str::from_utf8(&u)?;
    let v = u8::from_str_radix(s, 10)?;
    Ok((v, rest))
}

// NOTE: This is strictly ASCII.
// `reg-name = *( unreserved / pct-encoded / sub-delims )`
parser!(parse_reg_name<AsciiString> => {
    map(many(alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims
    )), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

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

// NOTE: This is strictly ASCII.
// `path-abempty = *( "/" segment )`
parser!(parse_path_abempty<Vec<AsciiString>> => {
    many(seq!(c => {
        c.next(one_of("/"))?; // TODO
        c.next(parse_segment)
    }))
});

// NOTE: This is strictly ASCII.
// `path-absolute = "/" [ segment-nz *( "/" segment ) ]`
parser!(parse_path_absolute<Vec<AsciiString>> => {
    seq!(c => {
        c.next(one_of("/"))?; // TODO
        c.next(parse_path_rootless)
    })
});

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

// NOTE: This is strictly ASCII.
// `path-empty = 0<pchar>`
fn parse_path_empty(input: Bytes) -> ParseResult<()> {
    Ok(((), input.clone()))
}

// NOTE: This is strictly ASCII.
// `segment = *pchar`
parser!(pub parse_segment<AsciiString> => {
    map(many(parse_pchar),
        |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// NOTE: This is strictly ASCII.
// `segment-nz = 1*pchar`
parser!(parse_segment_nz<AsciiString> => {
    map(many1(parse_pchar),
        |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

// NOTE: This is strictly ASCII.
// `segment-nz-nc = 1*( unreserved / pct-encoded / sub-delims / "@" )
// 				; non-zero-length segment without any colon ":"`
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

// NOTE: This is strictly ASCII.
// `pchar = unreserved / pct-encoded / sub-delims / ":" / "@"`
parser!(parse_pchar<u8> => {
    alt!(
        parse_unreserved, parse_pct_encoded, parse_sub_delims, one_of(":@")
    )
});

// NOTE: This is strictly ASCII.
// `query = *( pchar / "/" / "?" )`
parser!(pub parse_query<AsciiString> => parse_fragment);

// NOTE: This is strictly ASCII.
// `fragment = *( pchar / "/" / "?" )`
parser!(parse_fragment<AsciiString> => {
    map(many(alt!(
        parse_pchar, one_of("/?")
    )), |s| unsafe { AsciiString::from_ascii_unchecked(Bytes::from(s)) })
});

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

// NOTE: This is strictly ASCII.
// `unreserved = ALPHA / DIGIT / "-" / "." / "_" / "~"`
parser!(parse_unreserved<u8> => {
    like(|i: u8| {
        (i as char).is_alphanumeric() || i.is_one_of(b"-._~")
    })
});

// NOTE: This is strictly ASCII.
// NOTE: These must be 'pct-encoded' when appearing in a segment.
// `reserved = gen-delims / sub-delims`
fn is_reserved(i: u8) -> bool {
    is_gen_delims(i) || is_sub_delims(i)
}

// NOTE: This is strictly ASCII.
// `gen-delims = ":" / "/" / "?" / "#" / "[" / "]" / "@"`
fn is_gen_delims(i: u8) -> bool {
    i.is_one_of(b":/?#[]@")
}

fn is_sub_delims(i: u8) -> bool {
    i.is_one_of(b"!$&'()*+,;=")
}

// NOTE: This is strictly ASCII.
// `sub-delims = "!" / "$" / "&" / "'" / "(" / ")"
// 	           / "*" / "+" / "," / ";" / "="`
parser!(parse_sub_delims<u8> => like(is_sub_delims));

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
}
