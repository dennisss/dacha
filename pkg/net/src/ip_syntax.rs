// Functions for serializing and parsing IP addresses to/from human readable
// strings.
//
// TODO: Make this no_alloc in the parsing path.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt::Write;

use common::bytes::Bytes;
use common::errors::*;
use parsing::*;

use crate::ip::IPAddress;

type ParserInput<'a> = &'a [u8];

parser!(pub parse_ip<IPAddress> => {
    alt!(
        parse_ip_literal,
        map(parse_ipv4_address, |v| IPAddress::V4(*array_ref![v, 0, 4]))
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
            map(parse_ipv6_address, |v| IPAddress::V6(*array_ref![v, 0, 16])),
            // map(parse_ip_vfuture, |v| IPAddress::VFuture(v))
        ))?;
        c.next(one_of("]"))?;
        Ok(addr)
    })
});

// RFC 3986: Section 3.2.2
pub fn serialize_ip(value: &IPAddress) -> String {
    let s: String = match value {
        IPAddress::V4(v) => {
            format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3])
        }
        IPAddress::V6(v) => {
            // (start_index, end_index) of the longest range of zero bytes seen
            // in the address. Initialized to a range that is
            // trivially outside the address length.
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
            s.push('[');

            for (i, byte) in v.iter().enumerate() {
                if i >= longest_zero_range.0 && i < longest_zero_range.1 {
                    continue;
                }

                write!(&mut s, "{:02x}", byte).unwrap();
                if i < v.len() {
                    s.push(':');
                }
                if i + 1 == longest_zero_range.0 {
                    s.push(':');
                }
            }

            s.push(']');

            s
        } /* IPAddress::VFuture(v) => {
           *     return Err(err_msg("Serializing future IP formats not yet supported"));
           * } */
    };

    s

    // out.extend_from_slice(s.as_bytes());
    // Ok(())
}

/*
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
*/

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
fn parse_ipv6_address(input: &[u8]) -> ParseResult<Vec<u8>, &[u8]> {
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
fn parse_h16(input: &[u8]) -> ParseResult<Vec<u8>, &[u8]> {
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

    let mut decoded =
        base_radix::hex_decode(unsafe { core::str::from_utf8_unchecked(&padded) }).unwrap();
    if decoded.len() == 1 {
        decoded.insert(0, 0);
    }

    assert_eq!(decoded.len(), 2);

    Ok((decoded, &input[i..]))
}

/// RFC 3986: Section 3.2.2
///
/// `ls32 = ( h16 ":" h16 ) / IPv4address`
fn parse_ls32(input: &[u8]) -> ParseResult<Vec<u8>, &[u8]> {
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
fn parse_ipv4_address(input: &[u8]) -> ParseResult<Vec<u8>, &[u8]> {
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
fn parse_dec_octet(input: &[u8]) -> ParseResult<u8, &[u8]> {
    // TODO: Validate only taking 3 characters.

    let (u, rest) = take_while1(|i: u8| (i as char).is_digit(10))(input)?;
    let s = std::str::from_utf8(&u)?;
    let v = u8::from_str_radix(s, 10)?;
    Ok((v, rest))
}

/// RFC 3986: Section 3.2.3
///
/// `port = *DIGIT`
pub fn parse_port(input: &[u8]) -> ParseResult<Option<u16>, &[u8]> {
    let (v, rest) = take_while1(|i| (i as char).is_digit(10))(input)?;
    if v.len() == 0 {
        return Ok((None, rest));
    }
    let s = std::str::from_utf8(&v)?;
    let p = u16::from_str_radix(s, 10)?;
    Ok((Some(p), rest))
}
