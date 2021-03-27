use common::errors::*;
use parsing::*;

use crate::header::*;
use crate::transfer_encoding::*;
use crate::header_syntax::*;
use crate::common_syntax::*;
use crate::message_syntax::*;

pub const MAX_TRANSFER_CODINGS: usize = 4;

/// RFC 7230: Section 3.3.1
/// 
/// TODO: Must tolerate empty items in comma delimited lsit
pub fn parse_transfer_encoding(headers: &HttpHeaders) -> Result<Vec<TransferCoding>> {
    let mut out = vec![];
    for h in headers.find(TRANSFER_ENCODING) {
        let (items, _) = complete(comma_delimited(
            parse_transfer_coding,
            1,
            MAX_TRANSFER_CODINGS,
        ))(h.value.data.clone())?;

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




