// Shared utilities between the inflate and deflate implementations.

use crate::huffman::*;
use common::bits::*;
use common::errors::*;

pub const BTYPE_NO_COMPRESSION: u8 = 0b00;
pub const BTYPE_FIXED_CODES: u8 = 0b01;
pub const BTYPE_DYNAMIC_CODES: u8 = 0b10;

pub const MIN_REFERENCE_DISTANCE: usize = 1;
/// Maximum distance behind the current output position that a reference can
/// refer to (aka there is no point in using a window buffer larger than this
/// size).
pub const MAX_REFERENCE_DISTANCE: usize = 32768;

pub const MIN_REFERENCE_LENGTH: usize = 3;

/// The maximum allowed code length for encoding a literal/length symbol.
pub const MAX_LITLEN_CODE_LEN: usize = 15;

pub const INFLATE_EARLY_END: &'static str = "End of stream before final block";

pub const END_OF_BLOCK: usize = 256;

/// Number of distint symbols in the code length alphabet (0-19)
/// - 0-15 represent code lengths.
/// - 16,17,18 represent some form of repetition.
pub const CODE_LEN_ALPHA_SIZE: usize = 19;

/// The code length huffman tree that encodes code lengths is serialized as an
/// array of code lengths where the code lengths for specific symbols are
/// written out in this order.
pub const CODE_LEN_CODE_LEN_ORDERING: [u8; CODE_LEN_ALPHA_SIZE] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// The inverted zersion of above
// pub const CODE_LEN_CODE_LEN_INV_ORDERING: [u8; CODE_LEN_ALPHA_SIZE] = [

// ];

#[derive(Debug)]
pub struct Reference {
    pub distance: usize,
    pub length: usize,
}

// Fixed tree for literal/length alphabet to be used if no dynamic tree is
// specified.
pub fn fixed_huffman_lenlit_tree() -> Result<HuffmanTree> {
    let mut lens = vec![];
    for i in 0..144 {
        lens.push(8);
    }
    for i in 144..256 {
        lens.push(9);
    }
    for i in 256..280 {
        lens.push(7);
    }
    for i in 280..288 {
        lens.push(8);
    }

    HuffmanTree::from_canonical_lens(&lens)
}

// Fixed
pub fn fixed_huffman_dist_tree() -> Result<HuffmanTree> {
    let mut lens = vec![];
    lens.resize(32, 5);

    HuffmanTree::from_canonical_lens(&lens)
}

// TODO: Will also need an encoding version
pub fn read_len(code: usize, strm: &mut BitReader) -> Result<usize> {
    Ok(match code {
        257..=264 => (code - 257 + 3),
        265..=268 => {
            let b = strm.read_bits_exact(1)?;
            2 * (code - 265) + b + 11
        }
        269..=272 => {
            let b = strm.read_bits_exact(2)?;
            4 * (code - 269) + b + 19
        }
        273..=276 => {
            let b = strm.read_bits_exact(3)?;
            8 * (code - 273) + b + 35
        }
        277..=280 => {
            let b = strm.read_bits_exact(4)?;
            16 * (code - 277) + b + 67
        }
        281..=284 => {
            let b = strm.read_bits_exact(5)?;
            32 * (code - 281) + b + 131
        }
        285 => 258,
        _ => {
            return Err(err_msg("Invalid length code"));
        }
    })
}

pub fn append_lit(val: u8, out: &mut Vec<Atom>) -> Result<()> {
    out.push(Atom::LitLenCode(val as usize));
    Ok(())
}

pub fn append_end_of_block(out: &mut Vec<Atom>) {
    out.push(Atom::LitLenCode(END_OF_BLOCK));
}

const BITS_PER_BYTE: usize = 8;

/// Integer log_2(x). Returns the floor of the exact answer.
///
/// Should be implemented as 2 instructions on most processors.
pub fn log2(v: usize) -> usize {
    assert!(v > 0);
    (BITS_PER_BYTE * std::mem::size_of::<usize>() - 1) - (v.leading_zeros() as usize)
}

pub fn append_len(len: usize, out: &mut Vec<Atom>) -> Result<()> {
    if len < MIN_REFERENCE_LENGTH || len > 258 {
        return Err(err_msg("Length out of allowed range"));
    }

    match len {
        3..=10 => {
            out.push(Atom::LitLenCode(254 + len));
        }
        258 => {
            out.push(Atom::LitLenCode(285));
        }
        _ => {
            // These are 4 codes per extra bits size, so 2 bits of information are captured
            // by the code that we subtract from the code.
            let nbits = log2(len - MIN_REFERENCE_LENGTH) - 2;
            let mul = 1 << nbits; // 2^nbits
            let start = (mul << 2) + MIN_REFERENCE_LENGTH; // (2^(nbits + 2) + min)
            println!("N {}  MUL {} START {} LEN {}", nbits, mul, start, len);
            let extra = (len - start) % mul;
            let code = ((len - start) / mul) + (261 + 4 * nbits);

            out.push(Atom::LitLenCode(code));
            out.push(Atom::ExtraBits(BitVector::from_usize(extra, nbits as u8)));
        }
    }

    Ok(())
}

pub fn read_distance(code: usize, strm: &mut BitReader) -> Result<usize> {
    if code <= 3 {
        Ok(code + 1)
    } else if code <= 29 {
        let nbits = ((code - 4) / 2) + 1;
        let mul = 1 << nbits;
        let start = (mul << 1) + 1;
        let b = strm.read_bits_exact(nbits as u8)?;

        Ok(mul * (code % 2) + start + b)
    } else {
        Err(err_msg("Invalid distance code"))
    }
}

pub fn append_distance(dist: usize, out: &mut Vec<Atom>) -> Result<()> {
    if dist < 1 || dist > 32768 {
        return Err(err_msg("Distance out of allowed range"));
    }

    if dist <= 4 {
        let code = dist - 1;
        out.push(Atom::DistCode(code));
    // No extra bits
    } else {
        // TODO: Simplify this
        let nbits = log2(dist - MIN_REFERENCE_DISTANCE) - 1;
        let mul = 1 << nbits;
        let start = (mul << 1) + MIN_REFERENCE_DISTANCE;

        // TODO: Check this
        let extra = (dist - start) % mul;
        let code = ((dist - start) / mul) + 2 * (nbits + 1);

        out.push(Atom::DistCode(code));
        out.push(Atom::ExtraBits(BitVector::from_usize(extra, nbits as u8)));
    }

    Ok(())
}

/// A single atomic unit of a block.
#[derive(Debug, PartialEq)]
pub enum Atom {
    /// A literal/length code from 0-285 to be encoded using the corresponding
    /// huffman tree.
    LitLenCode(usize),
    /// A distance code from 0-29 to be encoded using the corresponding huffman
    /// code
    DistCode(usize),
    /// To be stored as plain unencoded bits in the block.
    ExtraBits(BitVector),
}

use std::convert::TryInto;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log2_test() {
        assert_eq!(log2(1), 0);
        assert_eq!(log2(8), 3);
        assert_eq!(log2(345), 8);
    }

    #[test]
    fn read_len_test() {
        let try_with = |code, extra| {
            let data = vec![extra];
            let mut c = std::io::Cursor::new(data);
            let mut strm = BitReader::new(&mut c);
            read_len(code, &mut strm).unwrap()
        };

        assert_eq!(try_with(257, 0), 3);
        assert_eq!(try_with(266, 0b1), 14);
        assert_eq!(try_with(270, 0b10), 25);
    }

    #[test]
    fn read_distance_test() {
        let try_with = |code, extra, extra2| {
            let data = vec![extra, extra2];
            let mut c = std::io::Cursor::new(data);
            let mut strm = BitReader::new(&mut c);
            read_distance(code, &mut strm).unwrap()
        };

        assert_eq!(try_with(2, 0, 0), 3);
        assert_eq!(try_with(8, 0b011, 0), 20);
        assert_eq!(try_with(9, 0b010, 0), 27);
    }

    #[test]
    fn append_distance_test() {
        let mut out = vec![];
        append_distance(5, &mut out).unwrap();
        assert_eq!(
            &out,
            &[Atom::DistCode(4), Atom::ExtraBits("0".try_into().unwrap())]
        );

        out.clear();
        append_distance(30, &mut out).unwrap();
        assert_eq!(
            &out,
            &[
                Atom::DistCode(9),
                Atom::ExtraBits("101".try_into().unwrap())
            ]
        );

        out.clear();
        append_distance(3, &mut out).unwrap();
        assert_eq!(&out, &[Atom::DistCode(2)]);
    }
}
