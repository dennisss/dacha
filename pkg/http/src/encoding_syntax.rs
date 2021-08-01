use common::errors::*;
use parsing::ascii::AsciiString;
use parsing::*;

use crate::common_syntax::*;
use crate::encoding::*;
use crate::header::*;
use crate::header_syntax::*;
use crate::message_syntax::*;

pub const MAX_TRANSFER_CODINGS: usize = 4;

const MAX_CONTENT_ENCODINGS: usize = 4;

/// RFC 7230: Section 3.3.1
///
/// TODO: Must tolerate empty items in comma delimited lsit
pub fn parse_transfer_encoding(headers: &Headers) -> Result<Vec<TransferCoding>> {
    let mut out = vec![];
    for h in headers.find(TRANSFER_ENCODING) {
        let (items, _) = complete(comma_delimited(
            parse_transfer_coding,
            1,
            MAX_TRANSFER_CODINGS,
        ))(h.value.to_bytes())?;

        out.reserve(items.len());
        for i in items.into_iter() {
            out.push(i);
        }

        if out.len() > MAX_TRANSFER_CODINGS {
            return Err(err_msg("Too many Transfer-Codings"));
        }
    }

    Ok(out)
}

// RFC 7230: Section 4
//
// `transfer-coding = "chunked" / "compress"
//					/ "deflate" / "gzip" / transfer-extension`
parser!(parse_transfer_coding<TransferCoding> => {
    parse_transfer_extension
});

// RFC 7230: Section 4
//
// `transfer-extension = token *( OWS ";" OWS transfer-parameter )`
parser!(parse_transfer_extension<TransferCoding> => {
    seq!(c => {
        let name = c.next(parse_token)?.to_string();
        let params = c.many(seq!(c => {
            c.next(parse_ows)?;
            c.next(one_of(";"))?;
            c.next(parse_ows)?;
            Ok(c.next(parse_transfer_parameter)?)
        }));

        Ok(TransferCoding {
            raw_name: name, params
        })
    })
});

// RFC 7230: Section 4
//
// `transfer-parameter = token BWS "=" BWS ( token / quoted-string )`
parser!(parse_transfer_parameter<(String, String)> => {
    seq!(c => {
        let name = c.next(parse_token)?.to_string();
        c.next(parse_bws)?;
        c.next(one_of("="))?;
        c.next(parse_bws)?;
        let value = c.next(alt!(
            map(parse_token, |s| s.to_string()),
            map(parse_quoted_string, |s| s.to_string())
        ))?;
        Ok((name, value))
    })
});

/// This will return a list of all content encodings in the message.
/// Will result an empty list if there are no Content-Encoding headers.
/// For simplicity they will all be lowercased as
pub fn parse_content_encoding(headers: &Headers) -> Result<Vec<String>> {
    // TODO: Deduplicate this code with parse_transfer_encoding
    let mut out = vec![];
    for h in headers.find(CONTENT_ENCODING) {
        let (items, _) = complete(comma_delimited(
            parse_content_coding,
            1,
            MAX_CONTENT_ENCODINGS,
        ))(h.value.to_bytes())?;

        out.reserve(items.len());
        for i in items.into_iter() {
            out.push(i.to_string().to_ascii_lowercase());
        }

        if out.len() > MAX_CONTENT_ENCODINGS {
            return Err(err_msg("Too many Transfer-Codings"));
        }
    }

    Ok(out)
}

parser!(parse_content_coding<AsciiString> => parse_token);
