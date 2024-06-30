use common::errors::*;

pub struct KeyEncoder {}

impl KeyEncoder {
    // NOTE: encode_bytes(A) is NOT a prefix of encode_bytes(A + B)
    pub fn encode_bytes(input: &[u8], out: &mut Vec<u8>) {
        // 'ab\x0\x0'
        // 'ab\x0\xff\x0\x0'

        out.reserve(input.len() + 2);
        for b in input.iter().cloned() {
            if b == 0 {
                // Zero encoding
                out.extend_from_slice(&[0, 0xff]);
            } else {
                out.push(b);
            }
        }

        // Terminator.
        out.extend_from_slice(&[0, 0]);
    }

    pub fn decode_bytes<'a>(mut input: &'a [u8]) -> Result<(Vec<u8>, &'a [u8])> {
        let mut out = vec![];
        while input.len() > 0 {
            let b = input[0];
            input = &input[1..];

            if b == 0 {
                if input.is_empty() {
                    return Err(err_msg("Incomplete escape sequence"));
                }

                let b2 = input[0];
                input = &input[1..];

                if b2 == 0 {
                    return Ok((out, input));
                } else if b2 == 0xff {
                    out.push(0);
                } else {
                    return Err(err_msg("Unknown escaped character"));
                }
            } else {
                out.push(b);
            }
        }

        Err(err_msg("Value not terminated"))
    }

    pub fn encode_end_bytes(input: &[u8], out: &mut Vec<u8>) {
        out.extend_from_slice(input);
    }

    pub fn decode_end_bytes<'a>(input: &'a [u8]) -> Result<(&'a [u8], &'a [u8])> {
        Ok((input, &[]))
    }

    /*
    TODO:
    Representing signed integers:
    - First bit is the sign (0 if negative, 1 is positive)
    - Then we can use regular integer encoding (possibly inverting the value if it is negative)

    */

    pub fn encode_varuint(mut value: u64, inverted: bool, out: &mut Vec<u8>) {
        // Minimum number of bytes needed to encode the unsigned integer.
        let nbits = (64 - value.leading_zeros()) as usize;

        // Number of bytes needed to encode the variable length integer.
        let mut nbytes = std::cmp::max(1, common::ceil_div(nbits, 8));

        // We need 1 extra prefix bit for each byte in the varint.
        while nbytes < 9 && nbytes * 8 < nbits + nbytes {
            nbytes += 1;
        }

        let out_slice = {
            let l = out.len();
            out.resize(l + nbytes, 0);
            &mut out[l..]
        };

        if inverted {
            value = !value;
        }

        // Write out the integer in big endian order (starting with the LSB).
        for i in (0..nbytes).rev() {
            out_slice[i] = value as u8;
            value >>= 8;
        }

        // Write the prefix.
        // This will be a sequence of 1 bits starting at the MSB of length nbytes - 1
        // followed by a 1 bit. e.g. if the integer requires 3 bytes to encode,
        // the prefix will be '110.....'
        let prefix: u8 = !((1u16 << (9 - nbytes)) - 1) as u8;
        if inverted {
            out_slice[0] &= !prefix;
        } else {
            out_slice[0] |= prefix;
        }
    }

    pub fn decode_varuint(input: &[u8], inverted: bool) -> Result<(u64, &[u8])> {
        if input.len() == 0 {
            return Err(err_msg("Expected at least one byte for varuint"));
        }

        let mut first_byte = input[0];
        if inverted {
            first_byte = !first_byte;
        }

        let nbytes = (first_byte.leading_ones() + 1) as usize;
        if input.len() < nbytes {
            return Err(err_msg("Not enough bytes"));
        }

        // Get rid of the prefix
        if nbytes >= 8 {
            first_byte = 0;
        } else {
            first_byte = (first_byte << nbytes) >> nbytes;
        }

        let mut value = first_byte as u64;

        let input_slice = &input[0..nbytes];
        for i in 1..nbytes {
            // NOTE: This will never overflow as we can only encode up to 8 bytes worth of
            // data which is the size of a u64.
            value <<= 8;

            let mut byte = input_slice[i];
            // TODO: Optimize this to only invert at the very end?
            if inverted {
                byte = !byte;
            }

            value |= byte as u64;
        }

        Ok((value, &input[nbytes..]))
    }

    /*
    Other types to support:
    - Float
    - bool
    - String?
    - Fixed integer
    - Signed varint
    */
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Needs tests to verify bytes stability for this.

    fn run_encode_varuint_test(inverted: bool) {
        let mut out = vec![];

        // Testing most 1-3 byte numbers
        for i in 0..100_000 {
            out.clear();
            KeyEncoder::encode_varuint(i, inverted, &mut out);

            let (j, rest) = KeyEncoder::decode_varuint(&out, inverted).unwrap();
            assert_eq!(j, i);
            assert_eq!(rest, &[]);
        }

        {
            out.clear();
            KeyEncoder::encode_varuint(0xffffffffffffffff, inverted, &mut out);
            let (j, rest) = KeyEncoder::decode_varuint(&out, inverted).unwrap();
            assert_eq!(j, 0xffffffffffffffff);
            assert_eq!(rest, &[]);
        }

        let ordered_nums = &[
            0, 1, 2, 3, 4, 5, 6, 10, 20, 21, 25, 26, 100, 255, 256, 5000, 5001, 10000, 65000,
            65001, 66000, 1000000, 2000000, 32434235,
        ];

        let mut out2 = vec![];
        for i in 0..ordered_nums.len() {
            out.clear();
            KeyEncoder::encode_varuint(ordered_nums[i], inverted, &mut out);

            for j in (i + 1)..ordered_nums.len() {
                out2.clear();
                KeyEncoder::encode_varuint(ordered_nums[j], inverted, &mut out2);

                if inverted {
                    assert!(out > out2, "{} > {}", ordered_nums[i], ordered_nums[j]);
                } else {
                    assert!(out < out2, "{} < {}", ordered_nums[i], ordered_nums[j]);
                }
            }
        }
    }

    #[test]
    fn encode_varuint_test() {
        run_encode_varuint_test(false);
        run_encode_varuint_test(true);
    }
}
