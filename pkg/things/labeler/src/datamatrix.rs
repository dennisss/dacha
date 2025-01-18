/*
Data Matrix Structure:
- Overall has an even number of rows / columns
- Quiet zone around the code.
- Left/bottom column/row black
- right/top column/row are alternating colors to form a timing pattern.

Some terminology:

- 'symbol': The entire datamatrix image (consists of a grid of 'modules').
- 'module': A single black or white square in the symbol.
- 'data codeword': A byte of data storing user data that will be packing into modules
- 'error codeword': A byte of reed solomon error correction code to be packed into modules.

*/

use base_error::*;
use image::BinaryImage;
use storage::erasure::galois::GaloisField8Bit;

/// NOTE: Actual min is 1 but 2 is the minimum recommended.
pub const MIN_DATAMATRIX_MARGIN: usize = 2;

const GF_POLY: u8 = 45;
const PAD_CODEWORD: u8 = 129;

/// Number of rows/columns used for the solid finder and timing pattern
const FINDER_PADDING: usize = 2;

struct SymbolSize {
    /// Number of rows/columns in the data section (doesn't include the
    /// finder/timing/quiet pattern).
    data_dim: usize,

    /// Number of user data code words that are stored in this symbol (including
    /// padding).
    data_words: usize,

    /// Number of error correction code words that are stored in this symbol.
    error_words: usize,
}

/// All the symbol sizes we support
const SIZES: &'static [SymbolSize] = &[
    SymbolSize {
        data_dim: 8,
        data_words: 3,
        error_words: 5,
    },
    SymbolSize {
        data_dim: 10,
        data_words: 5,
        error_words: 7,
    },
    SymbolSize {
        data_dim: 12,
        data_words: 8,
        error_words: 10,
    },
    SymbolSize {
        data_dim: 14,
        data_words: 12,
        error_words: 12,
    },
    SymbolSize {
        data_dim: 16,
        data_words: 18,
        error_words: 14,
    },
    SymbolSize {
        data_dim: 18,
        data_words: 22,
        error_words: 18,
    },
    SymbolSize {
        data_dim: 20,
        data_words: 30,
        error_words: 20,
    },
    SymbolSize {
        data_dim: 22,
        data_words: 36,
        error_words: 24,
    },
    SymbolSize {
        data_dim: 24,
        data_words: 44,
        error_words: 28,
    },
];

/// Creates an EC200 Datamatrix code.
///
/// In the output image zeros are white. Ones are black.
///
/// WARNING: The returned image must be surrounding by at least 2-4 units of all
/// sides of white 'margin' (not included in the returned image).
///
/// Limitations:
/// - Can only make square codes with 1 data region.
/// - Only ASCII data can be encoded (naively in 8 bits per character).
pub fn encode_datamatrix(data: &[u8]) -> Result<BinaryImage> {
    // Find the smallest symbol size that can fit all the data.
    //
    // TODO: Eventually support non-ASCII encoding methods to try and further
    // compress the data.
    let symbol_size = SIZES
        .iter()
        .find(|size| size.data_words >= data.len())
        .ok_or_else(|| {
            rpc::Status::invalid_argument(format!(
                "No symbol size supported that can fit {} bytes of data",
                data.len()
            ))
        })?;

    let margin_size = 0;

    // Contains all the combined code words we will encode.
    let mut chars = vec![];
    chars.reserve(symbol_size.data_words + symbol_size.error_words);

    for v in data.iter().cloned() {
        if v >= 128 {
            return Err(
                rpc::Status::invalid_argument("Only ASCII characters can be encoded").into(),
            );
        }

        chars.push(v + 1);
    }

    pad_with_253randomizer(&mut chars, symbol_size.data_words);

    chars.extend(reed_solomon_encode(&chars[..], symbol_size.error_words));

    let img_size = symbol_size.data_dim + FINDER_PADDING + (2 * margin_size);
    let mut img = BinaryImage::zero(img_size, img_size);

    for i in 0..(symbol_size.data_dim + FINDER_PADDING) {
        // Left finder line.
        img.set(margin_size + i, margin_size, 1);

        // Bottom finder line
        img.set(img_size - 1 - margin_size, margin_size + i, 1);

        // Right timing pattern (alternating).
        img.set(margin_size + i, img_size - 1 - margin_size, (i % 2) as u8);

        // Top timing pattern (alternating)
        img.set(margin_size, margin_size + i, ((i + 1) % 2) as u8);
    }

    {
        let mut filler = CharacterTiler {
            data: &chars[..],
            data_i: 0,
            array: vec![0u8; symbol_size.data_dim * symbol_size.data_dim],
            ncol: symbol_size.data_dim as isize,
            nrow: symbol_size.data_dim as isize,
        };

        filler.fill();

        assert_eq!(filler.data_i, chars.len());

        // Copy into the final image buffer.
        for y in 0..symbol_size.data_dim {
            for x in 0..symbol_size.data_dim {
                let v = filler.array[y * symbol_size.data_dim + x] & 1;
                img.set(margin_size + 1 + y, margin_size + 1 + x, v);
            }
        }
    }

    Ok(img)
}

fn pad_with_253randomizer(data: &mut Vec<u8>, target_len: usize) {
    if data.len() < target_len {
        data.push(PAD_CODEWORD); // First pad character is not randomized.
    }

    while data.len() < target_len {
        let pos = data.len() + 1;
        let num = ((149 * pos) % 253) + 1;
        let tmp = num + (PAD_CODEWORD as usize);

        let v = {
            if tmp <= 254 {
                tmp
            } else {
                tmp - 254
            }
        };

        data.push(v as u8);
    }
}

fn reed_solomon_encode(data: &[u8], error_length: usize) -> Vec<u8> {
    let mut gf = storage::erasure::galois::GaloisField8Bit::new(GF_POLY);

    // TODO: Better understand why this isn't equivalent to
    // 'VandermondReedSolomonEncoder'.

    let generator = reed_solomon_generator_poly(error_length, &gf);

    // 'out = data * 2^(8*error_length)'
    let mut out = data.to_vec();
    out.resize(data.len() + error_length, 0);

    // Polynomial long division computing just the remainder as described in
    // https://en.wikipedia.org/wiki/Polynomial_long_division
    //
    // 'out' contains the initial numerator and we iteratively subtract multiples of
    // 'generator' to make 'out' contain the remainder of 'out / generator'.
    for i in 0..data.len() {
        if out[i] == 0 {
            continue;
        }

        // 'out[i] / generator[0]' though generator[0] is 1.
        let scale = out[i];

        // NOTE: This could be make more efficient since we don't need to modify
        // out[i + 0] since it will become zero and won't be used anymore.
        for j in 0..generator.len() {
            out[i + j] = gf.sub(out[i + j], gf.mul(scale, generator[j]));
        }

        assert_eq!(out[i], 0);
    }

    out[data.len()..].to_vec()
}

/// Computes the polynomial '(x + 2^1) (x + 2^2) ... (x + 2^n)'
fn reed_solomon_generator_poly(n: usize, gf: &GaloisField8Bit) -> Vec<u8> {
    let mut poly = vec![0u8; n + 1];

    // Initialize polynomial to '1'
    poly[n] = 1;

    let mut carry = 0;

    // Each run of this loop multiplies poly by 'x + 2^i'
    for i in 1..(n + 1) {
        let mut carry = 0;

        let v = gf.pow(2, i as u8);

        for p in poly.iter_mut().rev() {
            // this is the '1x * p' term
            // we need to save it before we overwrite 'p'.
            let next_carry = *p;

            *p = gf.add(carry, gf.mul(*p, v));

            carry = next_carry;
        }

        assert_eq!(carry, 0);
    }

    assert_eq!(poly[0], 1);

    poly
}

/// Fills the symbol with 'L' shaped tiles where each tile contains all 8 bytes
/// of one codeword. The tiles are laid out in a weird zig zag pattern.
///
/// This is heavily derived from ISO/IEC 16022:2006 Annex F
struct CharacterTiler<'a> {
    data: &'a [u8],
    data_i: usize,

    array: Vec<u8>,

    ncol: isize,

    nrow: isize,
}

impl<'a> CharacterTiler<'a> {
    /// Bit used in 'array' to indicate that a position has a valid color (black
    /// or white) in it.
    const MARKED_BIT: u8 = 1 << 7;

    fn fill(&mut self) {
        /* Starting in the correct location for character #1, bit 8,... */
        let mut row: isize = 4;
        let mut col: isize = 0;

        loop {
            /* repeatedly first check for one of the special corner cases, then... */
            if (row == self.nrow) && (col == 0) {
                let c = self.next_chr();
                self.corner1(c);
            }
            if (row == self.nrow - 2) && (col == 0) && (self.ncol % 4) != 0 {
                let c = self.next_chr();
                self.corner2(c);
            }
            if (row == self.nrow - 2) && (col == 0) && (self.ncol % 8 == 4) {
                let c = self.next_chr();
                self.corner3(c);
            }
            if (row == self.nrow + 4) && (col == 2) && ((self.ncol % 8) == 0) {
                let c = self.next_chr();
                self.corner4(c);
            }

            /* sweep upward diagonally, inserting successive characters,... */
            loop {
                if (row < self.nrow)
                    && (col >= 0)
                    && (!self.array_index_marked(row * self.ncol + col))
                {
                    let c = self.next_chr();
                    self.utah(row, col, c);
                }

                row -= 2;
                col += 2;

                if !((row >= 0) && (col < self.ncol)) {
                    break;
                }
            }

            row += 1;
            col += 3;

            /* & then sweep downward diagonally, inserting successive characters,... */
            loop {
                if (row >= 0)
                    && (col < self.ncol)
                    && (!self.array_index_marked(row * self.ncol + col))
                {
                    let c = self.next_chr();
                    self.utah(row, col, c);
                }

                row += 2;
                col -= 2;

                if !((row < self.nrow) && (col >= 0)) {
                    break;
                }
            }

            row += 3;
            col += 1;

            /* ... until the entire array is scanned */
            if !(row < self.nrow || col < self.ncol) {
                break;
            }
        }

        /* Lastly, if the lower righthand corner is untouched, fill in fixed pattern */
        if !self.array_index_marked(self.nrow * self.ncol - 1) {
            self.array[(self.nrow * self.ncol - 1) as usize] = 1 | Self::MARKED_BIT;
            self.array[(self.nrow * self.ncol - self.ncol - 2) as usize] = 1 | Self::MARKED_BIT;
        }
    }

    fn next_chr(&mut self) -> u8 {
        let i = self.data_i;
        self.data_i += 1;
        self.data[i]
    }

    /* "module" places "chr+bit" with appropriate wrapping within array[] */
    fn module(&mut self, mut row: isize, mut col: isize, chr: u8, bit: usize) {
        if (row < 0) {
            row += self.nrow;
            col += 4 - ((self.nrow + 4) % 8);
        }
        if (col < 0) {
            col += self.ncol;
            row += 4 - ((self.ncol + 4) % 8);
        }

        let i = row * self.ncol + col;
        self.array[i as usize] = ((chr >> (8 - bit)) & 1) | Self::MARKED_BIT;
    }

    fn array_index_marked(&self, idx: isize) -> bool {
        self.array[idx as usize] & Self::MARKED_BIT != 0
    }

    /* "utah" places the 8 bits of a utah-shaped symbol character in ECC200 */
    fn utah(&mut self, row: isize, col: isize, chr: u8) {
        self.module(row - 2, col - 2, chr, 1);
        self.module(row - 2, col - 1, chr, 2);
        self.module(row - 1, col - 2, chr, 3);
        self.module(row - 1, col - 1, chr, 4);
        self.module(row - 1, col, chr, 5);
        self.module(row, col - 2, chr, 6);
        self.module(row, col - 1, chr, 7);
        self.module(row, col, chr, 8);
    }

    /* "cornerN" places 8 bits of the four special corner cases in ECC200 */
    fn corner1(&mut self, chr: u8) {
        self.module(self.nrow - 1, 0, chr, 1);
        self.module(self.nrow - 1, 1, chr, 2);
        self.module(self.nrow - 1, 2, chr, 3);
        self.module(0, self.ncol - 2, chr, 4);
        self.module(0, self.ncol - 1, chr, 5);
        self.module(1, self.ncol - 1, chr, 6);
        self.module(2, self.ncol - 1, chr, 7);
        self.module(3, self.ncol - 1, chr, 8);
    }

    fn corner2(&mut self, chr: u8) {
        self.module(self.nrow - 3, 0, chr, 1);
        self.module(self.nrow - 2, 0, chr, 2);
        self.module(self.nrow - 1, 0, chr, 3);
        self.module(0, self.ncol - 4, chr, 4);
        self.module(0, self.ncol - 3, chr, 5);
        self.module(0, self.ncol - 2, chr, 6);
        self.module(0, self.ncol - 1, chr, 7);
        self.module(1, self.ncol - 1, chr, 8);
    }

    fn corner3(&mut self, chr: u8) {
        self.module(self.nrow - 3, 0, chr, 1);
        self.module(self.nrow - 2, 0, chr, 2);
        self.module(self.nrow - 1, 0, chr, 3);
        self.module(0, self.ncol - 2, chr, 4);
        self.module(0, self.ncol - 1, chr, 5);
        self.module(1, self.ncol - 1, chr, 6);
        self.module(2, self.ncol - 1, chr, 7);
        self.module(3, self.ncol - 1, chr, 8);
    }

    fn corner4(&mut self, chr: u8) {
        self.module(self.nrow - 1, 0, chr, 1);
        self.module(self.nrow - 1, self.ncol - 1, chr, 2);
        self.module(0, self.ncol - 3, chr, 3);
        self.module(0, self.ncol - 2, chr, 4);
        self.module(0, self.ncol - 1, chr, 5);
        self.module(1, self.ncol - 3, chr, 6);
        self.module(1, self.ncol - 2, chr, 7);
        self.module(1, self.ncol - 1, chr, 8);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pad_with_253randomizer_test() {
        // 'Wikipedia' example

        let mut data = [88, 106, 108, 106, 113, 102, 101, 106, 98].to_vec();

        pad_with_253randomizer(&mut data, 12);

        assert_eq!(
            &data[..],
            &[88, 106, 108, 106, 113, 102, 101, 106, 98, 129, 251, 147]
        );
    }

    #[test]
    fn generator_poly_test() {
        let mut gf = storage::erasure::galois::GaloisField8Bit::new(GF_POLY);

        assert_eq!(
            reed_solomon_generator_poly(5, &gf),
            &[1, 62, 111, 15, 48, 228]
        );

        assert_eq!(
            reed_solomon_generator_poly(12, &gf),
            &[1, 242, 100, 178, 97, 213, 142, 42, 61, 91, 158, 153, 41]
        );

        assert_eq!(
            reed_solomon_generator_poly(28, &gf),
            &[
                1, 255, 93, 168, 233, 151, 120, 136, 141, 213, 110, 138, 17, 121, 249, 34, 75, 53,
                170, 151, 37, 174, 103, 96, 71, 97, 43, 231, 211
            ]
        );
    }

    #[test]
    fn reed_solomon_encode_test() {
        // 'Wikipedia' example

        let data: &[u8; 12] = &[88, 106, 108, 106, 113, 102, 101, 106, 98, 129, 251, 147];
        let error = reed_solomon_encode(data, 12);

        assert_eq!(
            &error[..],
            &[104, 216, 88, 39, 233, 202, 71, 217, 26, 92, 25, 232]
        );
    }

    #[test]
    fn full_datamatrix_test() {
        let img = encode_datamatrix(b"Wikipedia").unwrap();

        assert_eq!(
            img.raw(),
            &[
                170, 170, 178, 171, 213, 92, 173, 7, 210, 144, 166, 219, 247, 2, 147, 137, 163,
                236, 166, 147, 204, 178, 163, 167, 148, 242, 194, 109, 253, 66, 255, 255
            ]
        );
    }
}
