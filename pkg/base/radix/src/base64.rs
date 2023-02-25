use alloc::string::String;
use alloc::vec::Vec;

use crate::{DecodeRadixError, DecodeRadixErrorKind};

const STANDARD_ALPHABET: &'static [u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const STANDARD_ALPHABET_INVERSE: [u8; 256] = {
    let mut v = [255u8; 256];

    let mut i = 0;
    while i < STANDARD_ALPHABET.len() {
        v[STANDARD_ALPHABET[i] as usize] = i as u8;
        i += 1
    }

    v
};

const PADDING: char = '=';

/// Every 3 bytes is expanded into 4 characters.
pub fn base64_encode(data: &[u8]) -> String {
    let mut out = String::new();
    out.reserve_exact(base64_encoded_len(data.len()));

    for chunk in data.chunks(3) {
        let mut group24 = 0;
        group24 |= (chunk[0] as u32) << 16;
        if chunk.len() >= 2 {
            group24 |= (chunk[1] as u32) << 8;
        }
        if chunk.len() >= 3 {
            group24 |= chunk[2] as u32;
        }

        let n = chunk.len() + 1;

        for i in 0..n {
            let shift = 24 - (i + 1) * 6;
            let group6 = ((group24 >> shift) & 0b111111) as usize;

            let c = STANDARD_ALPHABET[group6];

            out.push(c as char);
        }

        for _ in n..4 {
            out.push(PADDING);
        }
    }

    out
}

pub fn base64_encoded_len(data_len: usize) -> usize {
    // TODO: Simplify as ceil_div(data_len * 4, 3)
    let v = data_len * 4;

    let mut out = v / 3;
    if v % 3 != 0 {
        out += 1;
    }

    out
}

pub fn base64_decode(data: &str) -> Result<Vec<u8>, DecodeRadixError> {
    if data.len() % 4 != 0 {
        return Err(DecodeRadixError {
            input_position: data.len(),
            kind: DecodeRadixErrorKind::InvalidNumberOfDigits,
        });
    }

    let mut out = vec![];
    for (chunk_i, chunk) in data.as_bytes().chunks(4).enumerate() {
        let mut group24 = 0;
        let mut paddings = 0;
        for i in 0..chunk.len() {
            let group6 = {
                if chunk[i] == PADDING as u8 {
                    paddings += 1;
                    0
                } else if paddings > 0 {
                    return Err(DecodeRadixError {
                        input_position: 4 * chunk_i + i,
                        kind: DecodeRadixErrorKind::UnsupportedDigit,
                    });
                } else {
                    STANDARD_ALPHABET_INVERSE[chunk[i] as usize]
                }
            };

            if (group6 as usize) >= STANDARD_ALPHABET.len() {
                return Err(DecodeRadixError {
                    input_position: 4 * chunk_i + i,
                    kind: DecodeRadixErrorKind::UnsupportedDigit,
                });
            }

            let shift = 24 - (i + 1) * 6;
            group24 |= (group6 as u32) << shift;
        }

        for i in 0..(3 - paddings) {
            out.push((group24 >> (24 - 8 * (i + 1)) & 0xFF) as u8);
        }

        // TODO: Check that the remainder of the group24 is zeros.
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encode_decode() {
        let examples: &[(&'static [u8], &'static str)] = &[
            (
                b"Many hands make light work.",
                "TWFueSBoYW5kcyBtYWtlIGxpZ2h0IHdvcmsu",
            ),
            (b"light w", "bGlnaHQgdw=="),
            (b"light wo", "bGlnaHQgd28="),
            (b"light wor", "bGlnaHQgd29y"),
        ];

        for (expected_decoded, expected_encoded) in examples.iter().cloned() {
            let encoded = base64_encode(expected_decoded);
            let decoded = base64_decode(expected_encoded).unwrap();

            assert_eq!(&encoded, &expected_encoded);
            assert_eq!(&decoded, &expected_decoded);
        }
    }
}
