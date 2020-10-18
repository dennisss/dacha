use crate::common_parser::*;
use crate::spec::*;
use crate::uri::*;
use crate::uri_parser::*;
use common::bytes::Bytes;
use common::errors::*;
use parsing::ascii::*;
use parsing::iso::*;
use parsing::*;

// Syntax RFC: https://tools.ietf.org/html/rfc7230
// ^ Key thing being that 8bits per character in ISO-... encoding.

// 1#element => element *( OWS "," OWS element )

// TODO: Check ahainst all erata: https://www.rfc-editor.org/errata_search.php?rfc=7230s

//////////////////

// TODO: "In the interest of robustness, servers SHOULD ignore any empty line(s) received where a Request-Line is expected. In other words, if the server is reading the protocol stream at the beginning of a message and receives a CRLF first, it should ignore the CRLF." - https://www.w3.org/Protocols/rfc2616/rfc2616-sec4.html

// TODO: See https://tools.ietf.org/html/rfc7230#section-6.7 for upgrade

//    Connection = *( "," OWS ) connection-option *( OWS "," [ OWS
//     connection-option ] )

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

// NOTE: This does not parse the body
// `HTTP-message = start-line *( header-field CRLF ) CRLF [ message-body ]`
parser!(pub parse_http_message_head<HttpMessageHead> => {
    seq!(c => {
        let start_line = c.next(parse_start_line)?;
        let raw_headers = c.next(many(seq!(c => {
            let h = c.next(parse_header_field)?;
            c.next(parse_crlf)?;
            Ok(h)
        })))?;

        c.next(parse_crlf)?;
        Ok(HttpMessageHead {
            start_line, headers: HttpHeaders::from(raw_headers)
        })
    })
});

// `HTTP-name = %x48.54.54.50 ; HTTP`
parser!(parse_http_name<()> => {
    map(tag("HTTP"), |_| ())
});

// `HTTP-version = HTTP-name "/" DIGIT "." DIGIT`
parser!(parse_http_version<HttpVersion> => {
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
        Ok(HttpVersion {
            major, minor
        })
    })
});

// TODO: Well known uri: https://tools.ietf.org/html/rfc8615

// `Host = uri-host [ ":" port ]`

//    TE = [ ( "," / t-codings ) *( OWS "," [ OWS t-codings ] ) ]
//    Trailer = *( "," OWS ) field-name *( OWS "," [ OWS field-name ] )
//    Transfer-Encoding = *( "," OWS ) transfer-coding *( OWS "," [ OWS
//     transfer-coding ] )

//    URI-reference = <URI-reference, see [RFC3986], Section 4.1>
//    Upgrade = *( "," OWS ) protocol *( OWS "," [ OWS protocol ] )

//    Via = *( "," OWS ) ( received-protocol RWS received-by [ RWS comment
//     ] ) *( OWS "," [ OWS ( received-protocol RWS received-by [ RWS
//     comment ] ) ] )

// `absolute-form = absolute-URI`
parser!(parse_absolute_form<Uri> => parse_absolute_uri);

// NOTE: This is strictly ASCII.
// `absolute-path = 1*( "/" segment )`
parser!(parse_absolute_path<Vec<AsciiString>> => {
    many1(seq!(c => {
        c.next(one_of(b"/"))?;
        c.next(parse_segment)
    }))
});

// `asterisk-form = "*"`
parser!(parse_asterisk_form<u8> => one_of(b"*"));

// `authority-form = authority`
parser!(parse_authority_form<Authority> => parse_authority);

//    comment = "(" *( ctext / quoted-pair / comment ) ")"

// `connection-option = token`
parser!(parse_connection_option<AsciiString> => {
    parse_token
});

// `ctext = HTAB / SP / %x21-27 ; '!'-'''
//     	  / %x2A-5B ; '*'-'['
//     	  / %x5D-7E ; ']'-'~'
//     	  / obs-text`

// TODO: Can't ever use this directly without doing many!
// ^ So don't make it public.

// NOTE: Errata exists for this
// TODO: See also https://tools.ietf.org/html/rfc8187
// It's not entirely clear if the header can be non-ASCII, but for now, we leave
// it to be ISO- `field-content = field-vchar [ 1*( SP / HTAB / field-vchar )
// field-vchar ]`
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

// NOTE: This is strictly ASCII.
// `field-name = token`
parser!(pub parse_field_name<AsciiString> => parse_token);

// TODO: Perform special error to client if we get obs-fold
// `field-value = *( field-content / obs-fold )`
parser!(parse_field_value<Latin1String> => {
    and_then(slice(many(alt!(
        parse_field_content, parse_obs_fold
    ))), |v| Latin1String::from_bytes(v))
});

// `field-vchar = VCHAR / obs-text`
fn is_field_vchar(i: u8) -> bool {
    is_vchar(i) || is_obs_text(i)
}

// `header-field = field-name ":" OWS field-value OWS`
parser!(pub parse_header_field<HttpHeader> => {
    seq!(c => {
        let name = c.next(parse_field_name)?;
        c.next(one_of(":"))?;
        c.next(parse_ows)?;
        let value = c.next(parse_field_value)?;
        c.next(parse_ows)?;
        Ok(HttpHeader { name, value })
    })
});

// `http-URI = "http:" "//" authority path-abempty [ "?" query ]`

// `message-body = *OCTET`

// `method = token`
parser!(parse_method<AsciiString> => parse_token);

// NOTE: Errata exists for this
// `obs-fold = OWS CRLF 1*( SP / HTAB )`
parser!(parse_obs_fold<Bytes> => {
    slice(seq!(c => {
        c.next(parse_ows)?;
        c.next(tag(b"\r\n"))?;
        c.next(take_while1(|i| is_sp(i) || is_htab(i)))?;
        println!("FOLD");
        Ok(())
    }))
});

// `origin-form = absolute-path [ "?" query ]`
parser!(parse_origin_form<(Vec<AsciiString>, Option<AsciiString>)> => {
    seq!(c => {
        let abspath = c.next(parse_absolute_path)?;
        let q = c.next(opt(seq!(c => {
            c.next(one_of(b"?"))?;
            c.next(parse_query)
        })))?;

        Ok((abspath, q))
    })
});

// `partial-URI = relative-part [ "?" query ]`

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

// `protocol-name = token`
parser!(parse_protocol_name<AsciiString> => parse_token);

// `protocol-version = token`
parser!(parse_protocol_version<AsciiString> => parse_token);

// `pseudonym = token`
parser!(parse_pseudonym<AsciiString> => parse_token);

// `rank = ( "0" [ "." *3DIGIT ] ) / ( "1" [ "." *3"0" ] )`

// TODO: Because of obs-text, this is not necessarily ascii
// `reason-phrase = *( HTAB / SP / VCHAR / obs-text )`
parser!(parse_reason_phrase<Latin1String> => {
    and_then(take_while(|i| is_htab(i) || is_sp(i) ||
                                is_vchar(i) || is_obs_text(i)),
        |v| Latin1String::from_bytes(v))
});

// `received-by = ( uri-host [ ":" port ] ) / pseudonym`

// `received-protocol = [ protocol-name "/" ] protocol-version`

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

// `start-line = request-line / status-line`
parser!(parse_start_line<StartLine> => {
    alt!(
        map(parse_request_line, |l| StartLine::Request(l)),
        map(parse_status_line, |l| StartLine::Response(l))
    )
});

// `status-code = 3DIGIT`
fn parse_status_code(input: Bytes) -> ParseResult<u16> {
    if input.len() < 3 {
        return Err(err_msg("status_code: input too short"));
    }
    let s = std::str::from_utf8(&input[0..3])?;
    let code = u16::from_str_radix(s, 10)?;
    Ok((code, input.slice(3..)))
}

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

//    t-codings = "trailers" / ( transfer-coding [ t-ranking ] )
//    t-ranking = OWS ";" OWS "q=" rank

// trailer-part = *( header-field CRLF )
// transfer-coding = "chunked" / "compress" / "deflate" / "gzip" /
//  transfer-extension
// transfer-extension = token *( OWS ";" OWS transfer-parameter )
// transfer-parameter = token BWS "=" BWS ( token / quoted-string )

//    uri-host = <host, see [RFC3986], Section 3.2.2>

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
