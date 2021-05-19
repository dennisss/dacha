// TLS specific helpers for parsing binary packets.

use common::bytes::Bytes;
use common::errors::*;
use parsing::binary::*;
use parsing::*;

pub const U8_LIMIT: usize = u8::max_value() as usize;
pub const U16_LIMIT: usize = u16::max_value() as usize;
pub const U24_LIMIT: usize = 1 << 24;
pub const U32_LIMIT: usize = u32::max_value() as usize;

pub fn exp2(v: usize) -> usize {
    1 << v
}

/// Creates a parser for a variable length vector of bytes.
///
/// The max_bytes will be used to determine how large the length field is. In TLS,
/// the minimum number of bytes required to store the max_length are used to encode
/// the length of the vector.  
pub fn varlen_vector(min_bytes: usize, max_bytes: usize) -> impl Parser<Bytes> {
    seq!(c => {
        let len =
            if max_bytes <= U8_LIMIT {
                c.next(as_bytes(be_u8))? as usize
            } else if max_bytes <= U16_LIMIT {
                c.next(as_bytes(be_u16))? as usize
            } else if max_bytes <= U24_LIMIT {
                c.next(as_bytes(be_u24))? as usize
            } else if max_bytes <= U32_LIMIT {
                c.next(as_bytes(be_u32))? as usize
            } else {
                panic!("Maximum length not supported");
            };
        if len < min_bytes || len > max_bytes {
            return Err(err_msg("Length out of allowed range"));
        }

        let data = c.next(take_exact(len))?;
        Ok(data)
    })
}

/// Encodes a byte vector using the length prefixed wire format defined by TLS. 
pub fn serialize_varlen_vector<F: FnMut(&mut Vec<u8>)>(
    min_bytes: usize,
    max_bytes: usize,
    out: &mut Vec<u8>,
    mut f: F,
) {
    let i = out.len();
    let n = if max_bytes <= U8_LIMIT {
        1
    } else if max_bytes <= U16_LIMIT {
        2
    } else if max_bytes <= U24_LIMIT {
        3
    } else if max_bytes <= U32_LIMIT {
        4
    } else {
        panic!("Maximum length not supported");
    };

    out.resize(i + n, 0);
    let ii = out.len();

    f(out);

    let size = out.len() - ii;

    // TODO: Instead we should return an error.
    assert!(size >= min_bytes && size <= max_bytes);

    match n {
        1 => {
            out[i] = size as u8;
        }
        2 => {
            *array_mut_ref![out, i, 2] = (size as u16).to_be_bytes();
        }
        3 => {
            *array_mut_ref![out, i, 3] = u24_to_be_bytes(size as u32);
        }
        4 => {
            *array_mut_ref![out, i, 4] = (size as u32).to_be_bytes();
        }
        _ => panic!("Should not happen"),
    };
}
