// Helpers for encoding/decoding the string literal and varint formats used in
// the HPACK binary format.
//
// For varints, we use 'usize' as these values are used upstream exclusively as
// buffer indexes, so there is no point in making them bigger.

use common::bits::BitIoError;
use common::bits::{BitWrite, BitWriter};
use common::errors::*;
use parsing::binary::be_u8;
use parsing::{parse_next, take_exact};

use crate::hpack::static_tables::*;

// NOTE: For a 64 bit integer, we can bound the number of bytes needed as
// 'ceil(64 / 7) = 10 bytes'
const MAX_VARINT_EXTRA_BYTES: usize = common::ceil_div(8 * std::mem::size_of::<usize>(), 7);

/// RFC 7541: Section 5.1
pub fn serialize_varint(mut value: usize, prefix_bits: usize, out: &mut Vec<u8>) {
    assert!(prefix_bits >= 1 && prefix_bits <= 8);
    // On 8-bit addressed systems, the '1 << prefix_bits' will overflow.
    assert!(std::mem::size_of::<usize>() > 1);

    // This is the prefix mask. Contains exactly 'prefix_bits' 1-bits
    let limit: usize = (1 << prefix_bits) - 1;

    if value < limit {
        out.push(value as u8);
        return;
    }

    value -= limit;
    out.push(limit as u8);

    // Loop while the value can't be represented in 7-bits.
    while value >= (1 << 7) {
        out.push((value as u8) | (1 << 7));
        value >>= 7;
    }

    // NOTE: Will not have the top bit set.
    out.push(value as u8);
}

/// RFC 7541: Section 5.1
pub fn parse_varint(mut input: &[u8], prefix_bits: usize) -> Result<(usize, &[u8])> {
    assert!(prefix_bits >= 1 && prefix_bits <= 8);
    // On 8-bit addressed systems, the '1 << prefix_bits' will overflow.
    assert!(std::mem::size_of::<usize>() > 1);

    let limit: usize = (1 << prefix_bits) - 1;

    let mut value = (parse_next!(input, be_u8) as usize) & limit;
    if value != limit {
        return Ok((value, input));
    }

    const LOWER7: usize = (1 << 7) - 1;

    let mut done = false;
    for i in 0..MAX_VARINT_EXTRA_BYTES {
        let next_byte = parse_next!(input, be_u8);

        // TODO: Technically the shift could also overflow.
        value = ((next_byte as usize) & LOWER7)
            .checked_shl(7 * (i as u32))
            .and_then(|v| value.checked_add(v))
            .ok_or_else(|| err_msg("Too large to fit in usize"))?;

        if (next_byte & (1 << 7)) == 0 {
            done = true;
            break;
        }
    }

    if !done {
        return Err(err_msg("Too large to fit in usize"));
    }

    Ok((value, input))
}

pub fn serialize_string_literal(data: &[u8], maybe_compress: bool, out: &mut Vec<u8>) {
    let first_i = out.len();

    // NOTE: We use a heuristic here to guess that small data up to 5 bytes in
    // length is probably not worth compressing.
    if maybe_compress && data.len() > 5 {
        // TODO: Refactor huffman_encode to abandon compression if it exceeds the length
        // of the input early.
        let compressed = huffman_encode(data);
        if compressed.len() < data.len() {
            serialize_varint(compressed.len(), 7, out);
            out[first_i] |= 1 << 7;
            out.extend_from_slice(&compressed);
            return;
        }
    }

    serialize_varint(data.len(), 7, out);
    out.extend_from_slice(&data);
}

// TODO: Limit the expanded size?
pub fn parse_string_literal(mut input: &[u8]) -> Result<(Vec<u8>, &[u8])> {
    let (first_byte, _) = be_u8(input)?;
    let huffman_coded = first_byte & (1 << 7) != 0;

    let len = {
        let (v, rest) = parse_varint(input, 7)?;
        input = rest;
        v
    };

    let raw_data = parse_next!(input, take_exact(len as usize));

    let data = {
        if huffman_coded {
            let mut out = vec![];

            let mut cursor = std::io::Cursor::new(raw_data);
            let mut reader = common::bits::BitReader::new_with_order(
                &mut cursor,
                common::bits::BitOrder::MSBFirst,
            );

            let tree = &*crate::hpack::static_tables::HUFFMAN_TREE;

            loop {
                match tree.read_code(&mut reader) {
                    Ok(value) => {
                        reader.consume();
                        out.push(value as u8)
                    }
                    Err(e) => {
                        if let Some(BitIoError::NotEnoughBits) = e.downcast_ref() {
                            break;
                        }

                        return Err(e);
                    }
                }
            }

            let padding = reader.into_unconsumed_bits();

            // All bytes must have been consumed. In the last byte, at least 1 bit
            // must have been consumed.
            if padding.len() > 7 || (raw_data.len() as u64 != cursor.position()) {
                return Err(err_msg("Too much padding in string literal"));
            }

            // All bits of the padding must be 1's (same as MSBs of the EOS symbol).
            for i in 0..padding.len() {
                if padding.get(i).unwrap() != 1 {
                    return Err(err_msg("Invalid padding"));
                }
            }

            out
        } else {
            raw_data.to_vec()
        }
    };

    Ok((data, input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_test() -> Result<()> {
        // Testing all 8-bit prefix values which are encoded in one byte.
        for i in 0..254 {
            let mut out = vec![];
            serialize_varint(i, 8, &mut out);
            assert_eq!(&out, &[i as u8]);

            let (v, rest) = parse_varint(&out, 8)?;
            assert_eq!(v, i);
            assert_eq!(rest, &[]);
        }

        // Too big to decode for any bit prefix
        {
            let too_big = [0xffu8; 20];
            for i in 1..=8 {
                assert!(parse_varint(&too_big, i).is_err());
            }
        }

        // High bits in the first byte shouldn't do anything.
        {
            let input = &[
                // 21
                0b11010101, // 56
                0xFF, 0b00011001,
            ];

            let (v1, rest1) = parse_varint(input, 5)?;
            assert_eq!(v1, 21);
            assert_eq!(rest1, &[0xff, 0b00011001]);

            let (v2, rest2) = parse_varint(rest1, 5)?;
            assert_eq!(v2, 56);
            assert_eq!(rest2, &[]);
        }

        let test_pair = |value: usize, nbits: usize, input: &[u8]| -> Result<()> {
            let mut out = vec![];
            serialize_varint(value, nbits, &mut out);
            assert_eq!(&out, input);

            // TODO: Test all incomplete variatiosn of the input.

            // TODO: Try intentionally adding padding to verify that 'rest' cuts to the
            // right spot.
            let (v, rest) = parse_varint(&out, nbits)?;
            assert_eq!(v, value);
            assert_eq!(rest, &[]);

            Ok(())
        };

        // RFC 7541: Appendix C.1.1
        test_pair(10, 5, &[0b00001010])?;
        // RFC 7541: Appendix C.1.2
        test_pair(1337, 5, &[0b00011111, 0b10011010, 0b00001010])?;
        // RFC 7541: Appendix C.1.3
        test_pair(42, 8, &[0b00101010])?;

        Ok(())
    }

    #[test]
    fn varint2_test() -> Result<()> {
        let mut out = vec![];

        serialize_varint(4097, 5, &mut out);
        assert_eq!(&out, &[31, 226, 31]);

        let (v, rest) = parse_varint(&out, 5)?;
        assert_eq!(rest.len(), 0);
        assert_eq!(v, 4097);

        Ok(())
    }
}
