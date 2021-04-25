use crate::incomplete_error;
use crate::ParseResult;

macro_rules! primitive_parser {
    ($name:ident, $t:ty, $from:ident) => {
        pub fn $name<'a>(input: &'a [u8]) -> ParseResult<$t, &'a [u8]> {
            const LEN: usize = std::mem::size_of::<$t>();
            if input.len() < LEN {
                return Err(incomplete_error());
            }

            let v = <$t>::$from(*array_ref![input, 0, LEN]);
            Ok((v, &input[LEN..]))
        }
    };
}

primitive_parser!(be_u16, u16, from_be_bytes);
primitive_parser!(be_i16, i16, from_be_bytes);
primitive_parser!(be_u32, u32, from_be_bytes);
primitive_parser!(be_i32, i32, from_be_bytes);
primitive_parser!(be_u64, u64, from_be_bytes);
primitive_parser!(be_i64, i64, from_be_bytes);
primitive_parser!(be_f32, f32, from_be_bytes);
primitive_parser!(be_f64, f64, from_be_bytes);

primitive_parser!(le_u16, u16, from_le_bytes);
primitive_parser!(le_i16, i16, from_le_bytes);
primitive_parser!(le_u32, u32, from_le_bytes);
primitive_parser!(le_i32, i32, from_le_bytes);
primitive_parser!(le_u64, u64, from_le_bytes);
primitive_parser!(le_i64, i64, from_le_bytes);
primitive_parser!(le_f32, f32, from_le_bytes);
primitive_parser!(le_f64, f64, from_le_bytes);

pub fn be_u8(input: &[u8]) -> ParseResult<u8, &[u8]> {
    if input.len() < 1 {
        return Err(incomplete_error());
    }

    let v = input[0];
    Ok((v, &input[1..]))
}

pub fn le_u8(input: &[u8]) -> ParseResult<u8, &[u8]> {
    be_u8(input)
}

pub fn be_u24(input: &[u8]) -> ParseResult<u32, &[u8]> {
    if input.len() < 3 {
        return Err(incomplete_error());
    }

    let mut buf = [0u8; 4];
    buf[1..4].copy_from_slice(&input[0..3]);

    let v = u32::from_be_bytes(buf);
    Ok((v, &input[3..]))
}

pub fn u24_to_be_bytes(v: u32) -> [u8; 3] {
    let buf = v.to_be_bytes();
    *array_ref![buf, 1, 3]
}
