use common::errors::*;
use parsing::*;

use crate::common_syntax::*;
use crate::header::*;
use crate::message_syntax::*;

// Content-Length: https://tools.ietf.org/html/rfc7230#section-3.3.2

// TODO: What to do about empty elements again?

// TODO: Must also ignore empty list items especially in the 1#element case
// ^ Parsing too many empty values may result in denial of service

/// Encodes http comma separated values as used in headers.
/// It is recommended to always explicitly define a maximum number of items.
///
/// Per RFC7230 Section 7, this will tolerate empty items.
///
/// In the RFCs, this corresponds to these grammar rules:
/// `1#element => element *( OWS "," OWS element )`
/// `#element => [ 1#element ]`
/// `<n>#<m>element => element <n-1>*<m-1>( OWS "," OWS element )`
pub fn comma_delimited<T, P: Parser<T> + Copy>(
    p: P,
    min: usize,
    max: usize,
) -> impl Parser<Vec<T>> {
    assert!(max >= min && max >= 1);

    seq!(c => {
        let mut out = vec![];
        out.push(c.next(p)?);
        while out.len() < max {
            let item = c.next(opt(seq!(c => {
                c.next(parse_ows)?;
                c.next(one_of(","))?;
                c.next(parse_ows)?;
                Ok(c.next(opt(p))?)
            })))?;

            match item {
                Some(Some(v)) => out.push(v),
                // In this case, we failed to parse (meaning most likely the
                // item is empty and will be skipped in the next loop).
                // NOTE: That behavior assumes that this is wrapped by a
                // complete()
                Some(None) => { continue; }
                None => { break; }
            };
        }

        if out.len() < min {
            return Err(err_msg("Too few items parsed"));
        }

        Ok(out)
    })
}

// A sender MUST
//    NOT apply chunked more than once to a message body (i.e., chunking an
//    already chunked message is not allowed).  If any transfer coding
//    other than chunked is applied to a request payload body, the sender
//    MUST apply chunked as the final transfer coding to ensure that the
//    message is properly framed.

/// RFC 7230: Section 3.3.2
///
/// `Content-Length = 1*DIGIT`
pub fn parse_content_length(headers: &Headers) -> Result<Option<usize>> {
    let mut hs = headers.find(CONTENT_LENGTH);
    let len = if let Some(h) = hs.next() {
        if let Ok(v) = usize::from_str_radix(h.value.to_ascii_str()?, 10) {
            Some(v)
        } else {
            return Err(format_err!("Invalid Content-Length: {:?}", h.value));
        }
    } else {
        // No header present.
        None
    };

    // Having more than one header is an error.
    if !hs.next().is_none() {
        return Err(err_msg("More than one Content-Length header received."));
    }

    Ok(len)
}
