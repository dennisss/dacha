use common::bytes::Bytes;
use common::errors::*;
use parsing::*;
use parsing::ascii::AsciiString;
use parsing::iso::Latin1String;

use crate::chunked::*;
use crate::header::*;
use crate::header_syntax;
use crate::message_syntax::*;
use crate::common_syntax::*;

// RFC 7230: Section 4.1.1
//
// `chunk-ext-name = token`
parser!(parse_chunk_ext_name<AsciiString> => parse_token);

// `chunk-ext-val = token / quoted-string`
parser!(parse_chunk_ext_val<Latin1String> => alt!(
    parse_quoted_string,
    and_then(parse_token, |s| Latin1String::from_bytes(s.data))
));

// RFC 7230: Section 4.1
//
// `= chunk-size [ chunk-ext ] CRLF`
parser!(pub parse_chunk_start<ChunkHead> => seq!(c => {
    let size = c.next(parse_chunk_size)?;
    let extensions = c.next(opt(parse_chunk_ext))?
        .unwrap_or(vec![]);
    c.next(parse_crlf)?;

    Ok(ChunkHead { size, extensions })
}));

// RFC 7230: Section 4.1
//
// `= trailer-part CRLF`
parser!(pub parse_chunk_end<Vec<Header>> => {
    seq!(c => {
        let headers = c.next(parse_trailer_part)?;
        c.next(parse_crlf)?;
        Ok(headers)
    })
});

// RFC 7230: Section 4.1
//
// `chunk-size = 1*HEXDIG`
// TODO: Ensure not out of range.
parser!(parse_chunk_size<usize> => {
    map(take_while1(|i: u8| (i as char).is_digit(16)),
        |data: Bytes| usize::from_str_radix(
            std::str::from_utf8(&data).unwrap(), 16).unwrap())
});

// RFC 7230: Section 4.1.1
//
// chunk-ext = *( ";" chunk-ext-name [ "=" chunk-ext-val ] )
parser!(parse_chunk_ext<Vec<ChunkExtension>> => {
    many(seq!(c => {
        c.next(one_of(";"))?;
        let name = c.next(parse_chunk_ext_name)?;
        let value = c.next(opt(seq!(c => {
            c.next(one_of("="))?;
            Ok(c.next(parse_chunk_ext_val)?)
        })))?;

        Ok(ChunkExtension { name, value })
    }))
});

// RFC 7230: Section 4.1.2
//
// `trailer-part = *( header-field CRLF )`
parser!(parse_trailer_part<Vec<Header>> => {
    many(parse_header_field)
});