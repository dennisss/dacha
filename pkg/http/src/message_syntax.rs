// Parsing for the HTTP message syntax <= v1.1
// This file closely follows:
// - RFC 1945 (HTTP 0.9 / 1.0)
// - RFC 7230 (HTTP 1.1)

use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::*;
use parsing::iso::*;
use parsing::opaque::OpaqueString;
use parsing::*;

use crate::common_syntax::*;
use crate::uri::*;
use crate::uri_syntax::*;
use crate::message::*;
use crate::header::*;

// Syntax RFC: https://tools.ietf.org/html/rfc7230
// ^ Key thing being that 8bits per character in ISO-... encoding.

// 1#element => element *( OWS "," OWS element )

// TODO: Check ahainst all erata: https://www.rfc-editor.org/errata_search.php?rfc=7230s

// TODO: "In the interest of robustness, servers SHOULD ignore any empty line(s) received where a Request-Line is expected. In other words, if the server is reading the protocol stream at the beginning of a message and receives a CRLF first, it should ignore the CRLF." - https://www.w3.org/Protocols/rfc2616/rfc2616-sec4.html

// TODO: See https://tools.ietf.org/html/rfc7230#section-6.7 for upgrade


//////////////////

/*
    TODO: Remaining challenge is what to do with request-target?

    origin-form (typical)
        "/hello/world?.."

    absolute-form (for proxying)
        "http://www.example.org/pub/WWW/TheProject.html"
    
    authority-form (only for CONNECT)
        "www.example.com:80"

        ^ I don't think this can be constructed from a URI?

    asterisk-form
        "*""

*/


// RFC 1945: Section 4.1
// Parser for the entire HTTP 0.9 request.
// `Simple-Request = "GET" SP Request-URI CRLF`
// TODO: Check https://www.ietf.org/rfc/rfc1945.txt for exactly what is allowed in the Uri are allowed
parser!(pub parse_simple_request<Uri> => {
    seq!(c => {
        c.next(tag("GET"))?;
        c.next(like(is_sp))?;
        let uri = c.next(parse_request_target)?.into_uri();
        c.next(parse_crlf)?;
        Ok(uri)
    })
});

// RFC 7230: Section 2.6
//
// `HTTP-version = HTTP-name "/" DIGIT "." DIGIT`
parser!(parse_http_version<Version> => {
    let digit = |input| {
        let (i, rest) = any(input)?;
        let v: [u8; 1] = [i];
        let s = std::str::from_utf8(&v)?;
        let d = u8::from_str_radix(s, 10)?;
        Ok((d, rest))
    };

    seq!(c => {
        c.next(parse_http_name)?;
        c.next(one_of(b"/"))?;
        let major = c.next(digit)?;
        c.next(one_of(b"."))?;
        let minor = c.next(digit)?;
        Ok(Version {
            major, minor
        })
    })
});

fn serialize_http_version(version: &Version, out: &mut Vec<u8>) -> Result<()> {
    if version.major > 10 || version.minor > 10 {
        return Err(err_msg("Version out of range"));
    }

    let s = format!("HTTP/{}.{}", version.major, version.minor);
    out.extend_from_slice(s.as_bytes());
    Ok(())
}

// RFC 7230: Section 2.6
//
// `HTTP-name = %x48.54.54.50 ; HTTP`
parser!(parse_http_name<()> => {
    map(tag("HTTP"), |_| ())
});

// RFC 7230: Section 2.7.1
//
// `http-URI = "http:" "//" authority path-abempty [ "?" query ] [ "#" fragment ]`
// 
// TODO: Must reject Uris with an empty host.

// RFC 7230: Section 2.7.2
//
// TODO: Errata exist for this.
//
// "https:" "//" authority path-abempty [ "?" query ] [ "#" fragment ]

// RFC 7230: Section 3
//
// NOTE: This does not parse the body
// `HTTP-message = start-line *( header-field CRLF ) CRLF [ message-body ]`
parser!(pub(crate) parse_http_message_head<HttpMessageHead> => {
    seq!(c => {
        let start_line = c.next(parse_start_line)?;
        let raw_headers = c.next(many(seq!(c => {
            let h = c.next(parse_header_field)?;
            c.next(parse_crlf)?;
            Ok(h)
        })))?;

        c.next(parse_crlf)?;
        Ok(HttpMessageHead {
            start_line, headers: Headers::from(raw_headers)
        })
    })
});

// RFC 7230: Section 3
//
// `start-line = request-line / status-line`
parser!(parse_start_line<StartLine> => {
    alt!(
        map(parse_request_line, |l| StartLine::Request(l)),
        map(parse_status_line, |l| StartLine::Response(l))
    )
});

// RFC 7230: Section 3.1.1
// 
// `request-line = method SP request-target SP HTTP-version CRLF`
parser!(parse_request_line<RequestLine> => {
    seq!(c => {
        let m = c.next(parse_method)?;
        c.next(sp)?;
        let t = c.next(parse_request_target)?;
        c.next(sp)?;
        let v = c.next(parse_http_version)?;
        c.next(parse_crlf)?;
        Ok(RequestLine { method: m, target: t, version: v })
    })
});

pub fn serialize_request_line(method: &AsciiString, uri: &Uri, version: &Version, out: &mut Vec<u8>) -> Result<()> {
    serialize_method(&method, out)?;
    out.push(b' ');
    {
        // TODO: Improve this.
        serialize_uri(uri, out)?;
    }
    out.push(b' ');
    serialize_http_version(version, out)?;
    out.extend_from_slice(b"\r\n");
    Ok(())
}


// RFC 7230: Section 3.1.1
//
// `method = token`
parser!(parse_method<AsciiString> => parse_token);

fn serialize_method(value: &AsciiString, out: &mut Vec<u8>) -> Result<()> {
    serialize_token(value, out)
}

// RFC 7230: Section 3.1.2
//
// `status-line = HTTP-version SP status-code SP reason-phrase CRLF`
parser!(parse_status_line<StatusLine> => {
    seq!(c => {
        let version = c.next(parse_http_version)?;
        c.next(sp)?;
        let s = c.next(parse_status_code)?;
        c.next(sp)?;
        let reason = c.next(parse_reason_phrase)?;
        c.next(parse_crlf)?;
        Ok(StatusLine { version, status_code: s, reason })
    })
});

pub(crate) fn serialize_status_line(line: &StatusLine, out: &mut Vec<u8>) -> Result<()> {
    serialize_http_version(&line.version, out)?;
    out.push(b' ');
    serialize_status_code(line.status_code, out)?;
    out.push(b' ');
    serialize_reason_phrase(&line.reason, out)?;
    out.extend_from_slice(b"\r\n");
    Ok(())
}

/// RFC 7230: Section 3.1.2
/// 
/// `status-code = 3DIGIT`
fn parse_status_code(input: Bytes) -> ParseResult<u16> {
    if input.len() < 3 {
        return Err(err_msg("status_code: input too short"));
    }
    let s = std::str::from_utf8(&input[0..3])?;
    let code = u16::from_str_radix(s, 10)?;
    Ok((code, input.slice(3..)))
}

fn serialize_status_code(value: u16, out: &mut Vec<u8>) -> Result<()> {
    if value < 100 || value > 999 {
        return Err(err_msg("Status code must be 3 digits long"));
    }

    let s = value.to_string();
    out.extend_from_slice(s.as_bytes());
    Ok(())
}


// RFC 7230: Section 3.1.2
//
// TODO: Because of obs-text, this is not necessarily ascii
// `reason-phrase = *( HTAB / SP / VCHAR / obs-text )`
//
// TODO: Implement as a regexp
parser!(parse_reason_phrase<OpaqueString> => {
    map(take_while(|i| is_htab(i) || is_sp(i) ||
                                is_vchar(i) || is_obs_text(i)),
        |v| OpaqueString::from(v))
});

fn serialize_reason_phrase(value: &OpaqueString, out: &mut Vec<u8>) -> Result<()> {
    out.reserve(value.len());
    for i in value.as_bytes().iter().cloned() {
        if is_htab(i) || is_sp(i) || is_vchar(i) || is_obs_text(i) {
            out.push(i);
        } else {
            return Err(err_msg("Invalid character in reason phrase"));
        }
    }

    Ok(())
}

// RFC 7230: Section 3.2
//
// TODO: Validate that based on section 3.2.4, whitespace between the field name and the colon is rejected.
//
// `header-field = field-name ":" OWS field-value OWS`
parser!(pub parse_header_field<Header> => {
    seq!(c => {
        let name = c.next(parse_field_name)?;
        c.next(one_of(":"))?;
        c.next(parse_ows)?;
        let value = c.next(parse_field_value)?;
        c.next(parse_ows)?;
        Ok(Header { name, value })
    })
});

pub fn serialize_header_field(header: &Header, out: &mut Vec<u8>) -> Result<()> {
    serialize_field_name(&header.name, out)?;
    out.extend_from_slice(b": ");
    serialize_field_value(&header.value, out);
    Ok(())
}

// RFC 7230: Section 3.2
//
// NOTE: This is strictly ASCII.
// `field-name = token`
parser!(pub parse_field_name<AsciiString> => parse_token);

fn serialize_field_name(value: &AsciiString, out: &mut Vec<u8>) -> Result<()> {
    serialize_token(value, out)
}

// RFC 7230: Section 3.2
//
// TODO: Perform special error to client if we get obs-fold
// According to section 3.2.4, obs-fold is only allowed if the media type is message/http.
//
// `field-value = *( field-content / obs-fold )`
parser!(parse_field_value<OpaqueString> => {
    map(slice(many(alt!(
        parse_field_content, parse_obs_fold
    ))), |v: Bytes| {
        // Re-generate without any obs-fold.
        // TODO: Ideally modify the original buffer in-place (or use an arena)
        let mut out = vec![];
        out.reserve_exact(v.len());
        serialize_field_value_internal(&v, &mut out);
        OpaqueString::from(out)
    })
});

fn serialize_field_value(value: &OpaqueString, out: &mut Vec<u8>) {
    out.reserve(value.len());

    // TODO: Not all bytes are valid.
    // TODO: NEed to validate that it matches field-content or obs-fold.

    serialize_field_value_internal(value.as_bytes(), out);
}

fn serialize_field_value_internal(data: &[u8], out: &mut Vec<u8>) {
    for byte in data.iter().cloned() {
        // Convert obs-fold before forwarding. This will also prevent \r\n\r\n for being
        // serialized in a header
        if byte == b'\r' || byte == b'\n' {
            out.push(b' ');
        } else {
            out.push(byte);
        }
    }
}

// RFC 7230: Section 3.2
//
// NOTE: Errata exists for this
// TODO: See also https://tools.ietf.org/html/rfc8187
// It's not entirely clear if the header can be non-ASCII, but for now, we leave
// it to be ISO-
//
// `field-content = field-vchar [ 1*( SP / HTAB / field-vchar ) field-vchar ]`
parser!(pub parse_field_content<Bytes> => {
    // TODO: No point in using a slice as field_value already has this. Avoid extra Bytes clones and instead return ()
    slice(seq!(c => {
        c.next(like(is_field_vchar))?;

        // TODO: The question here is whether or not a header value should be allowed to end in whitespace.
        c.next(many(seq!(c => {
            c.next(take_while1(|i| is_sp(i) || is_htab(i)))?;
            c.next(like(is_field_vchar))
        })))
    }))
});

/// RFC 7230: Section 3.2
///
/// `field-vchar = VCHAR / obs-text`
fn is_field_vchar(i: u8) -> bool {
    is_vchar(i) || is_obs_text(i)
}

// RFC 7230: Section 3.2
//
// NOTE: Errata exists for this
// `obs-fold = OWS CRLF 1*( SP / HTAB )`
parser!(parse_obs_fold<Bytes> => {
    slice(seq!(c => {
        c.next(parse_ows)?;
        c.next(tag(b"\r\n"))?;
        c.next(take_while1(|i| is_sp(i) || is_htab(i)))?;
        Ok(())
    }))
});

/// RFC 7230: Section 3.2.6
/// 
/// NOTE: This is strictly ASCII.
/// `token = 1*tchar`
pub fn parse_token(input: Bytes) -> ParseResult<AsciiString> {
    let (v, rest) = take_while1(is_tchar)(input)?;

    // This works because tchar will only ever access ASCII characters which
    // are a subset of UTF-8
    let s = unsafe { AsciiString::from_ascii_unchecked(v) };
    Ok((s, rest))
}

pub fn serialize_token(value: &AsciiString, out: &mut Vec<u8>) -> Result<()> {
    for byte in value.as_ref().bytes() {
        if !is_tchar(byte) {
            return Err(format_err!("Invalid character in token: 0x{:x}", byte));
        }
    }

    out.extend_from_slice(value.as_ref().as_bytes());
    Ok(())
}


/// RFC 7230: Section 3.2.6
/// 
/// NOTE: This is strictly ASCII.
/// `tchar = "!" / "#" / "$" / "%" / "&" / "'" / "*" / "+" / "-" / "." /
///  "^" / "_" / "`" / "|" / "~" / DIGIT / ALPHA`
fn is_tchar(i: u8) -> bool {
    (i as char).is_ascii_alphanumeric() || i.is_one_of(b"!#$%&'*+-.^_`|~")
}

// RFC 7230: Section 3.2.6
//
// `quoted-string = DQUOTE *( qdtext / quoted-pair ) DQUOTE`
parser!(pub parse_quoted_string<Latin1String> => seq!(c => {
    c.next(one_of("\""))?;
    let data = c.next(many(alt!(
        like(is_qdtext), parse_quoted_pair)))?;
    c.next(one_of("\""))?;

    Ok(Latin1String::from_bytes(data.into())?)
}));

/// RFC 7230: Section 3.2.6
//
/// `qdtext = HTAB / SP / "!" / %x23-5B ; '#'-'['
///         / %x5D-7E ; ']'-'~'
///         / obs-text`
fn is_qdtext(i: u8) -> bool {
    is_htab(i)
        || is_sp(i)
        || (i as char) == '!'
        || (i >= 0x22 && i <= 0x5b)
        || (i >= 0x5d && i <= 0x7e)
        || is_obs_text(i)
}

/// RFC 7230: Section 3.2.6
/// TODO: 128 to 159 are undefined in ISO-8859-1
/// (Obsolete) Text
/// `obs-text = %x80-FF`
fn is_obs_text(i: u8) -> bool {
    i >= 0x80 && i <= 0xff
}

// RFC 7230: Section 3.2.6
//
// `comment = "(" *( ctext / quoted-pair / comment ) ")"`

// RFC 7230: Section 3.2.6
//
// `ctext = HTAB / SP / %x21-27 ; '!'-'''
//     	  / %x2A-5B ; '*'-'['
//     	  / %x5D-7E ; ']'-'~'
//     	  / obs-text`

// RFC 7230: Section 3.2.6
//
// `quoted-pair = "\" ( HTAB / SP / VCHAR / obs-text )`
parser!(parse_quoted_pair<u8> => seq!(c => {
    c.next(one_of("\\"))?;
    Ok(c.next(like(|i| is_htab(i) || is_sp(i) || is_vchar(i) || is_obs_text(i)))?)
}));

// RFC 7230: Section 5.3
//
// `request-target = origin-form / absolute-form / authority-form /
// asterisk-form`
parser!(pub parse_request_target<RequestTarget> => {
    alt!(
        map(parse_origin_form, |(p, q)| RequestTarget::OriginForm(p, q)),
        map(parse_absolute_form, |u| RequestTarget::AbsoluteForm(u)),
        map(parse_authority_form, |a| RequestTarget::AuthorityForm(a)),
        map(parse_asterisk_form, |_| RequestTarget::AsteriskForm)
    )
});

// RFC 7230: Section 5.3.1
//
// `origin-form = absolute-path [ "?" query ]`
parser!(parse_origin_form<(Vec<OpaqueString>, Option<AsciiString>)> => {
    seq!(c => {
        let abspath = c.next(parse_absolute_path)?;
        let q = c.next(opt(seq!(c => {
            c.next(one_of(b"?"))?;
            c.next(parse_query)
        })))?;

        Ok((abspath, q))
    })
});

// RFC 7230: Section 5.3.2
//
// `absolute-form = absolute-URI`
parser!(parse_absolute_form<Uri> => parse_absolute_uri);

// RFC 7230: Section 5.3.3
//
// `authority-form = authority`
parser!(parse_authority_form<Authority> => parse_authority);

// RFC 7230: Section 5.3.4
//
// `asterisk-form = "*"`
parser!(parse_asterisk_form<u8> => one_of(b"*"));

// RFC 7230: Section 5.7.1
// TODO: Used for the Via header.
//
// `received-protocol = [ protocol-name "/" ] protocol-version`

// RFC 7230: Section 5.7.1
// TODO: Used for the Via header.
//
// `received-by = ( uri-host [ ":" port ] ) / pseudonym`

// RFC 7230: Section 5.7.1
// TODO: USed for the Via header.
//
// `pseudonym = token`
parser!(parse_pseudonym<AsciiString> => parse_token);


//////////////////


// TODO: Well known uri: https://tools.ietf.org/html/rfc8615

// RFC 7230: Section 5.4
//
// `Host = uri-host [ ":" port ]`
// `uri-host = <host, see [RFC3986], Section 3.2.2>`

//    TE = [ ( "," / t-codings ) *( OWS "," [ OWS t-codings ] ) ]
//    Trailer = *( "," OWS ) field-name *( OWS "," [ OWS field-name ] )
//    Transfer-Encoding = *( "," OWS ) transfer-coding *( OWS "," [ OWS
//     transfer-coding ] )

//    Upgrade = *( "," OWS ) protocol *( OWS "," [ OWS protocol ] )

//    Via = *( "," OWS ) ( received-protocol RWS received-by [ RWS comment
//     ] ) *( OWS "," [ OWS ( received-protocol RWS received-by [ RWS
//     comment ] ) ] )



// `absolute-path = 1*( "/" segment )`
parser!(parse_absolute_path<Vec<OpaqueString>> => {
    many1(seq!(c => {
        c.next(one_of(b"/"))?;
        c.next(parse_segment)
    }))
});


// TODO: Can't ever use this directly without doing many!
// ^ So don't make it public.

// `partial-URI = relative-part [ "?" query ]`

// `rank = ( "0" [ "." *3DIGIT ] ) / ( "1" [ "." *3"0" ] )`

//    t-codings = "trailers" / ( transfer-coding [ t-ranking ] )
//    t-ranking = OWS ";" OWS "q=" rank


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_field_value_test() {
        // TODO: Testing examples from here:
        // https://en.wikipedia.org/wiki/Uniform_Resource_Identifier#Examples

        let (v, rest) = parse_field_value(
            "CP=\"This is not a P3P policy! See g.co/p3phelp for more info.\"".into(),
        )
        .unwrap();

        println!("{:?}", v);
        assert_eq!(rest.as_ref(), &[]);
    }
}
