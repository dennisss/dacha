use crate::erasure::galois::*;

/// Augments data with Reed Solomon parity data which can be used to reconstruct
/// the original data given a small number of blocks were corrupted.
///
/// - A user provides 'n' words of data which they want protected
///     - A word is 8 bits.
/// - The encoder appends 'm' words of parity to protect it.
///     - Of the 'n + m' combined words, any 'm' can be lost while still being
///       able to recover the base 'n' data words.
/// - This CAN NOT repair unknown errors. The user must know which of the 'n +
///   m' words haven't been corrupted.
///
/// Internally, this is implemented as:
/// - We find an '(n + m) by n' matrix A by:
///     - Starting with a Vandermond matrix of the same shape where element
///       (i,j) is i^j
///     - Apply column swapping/scaling/linear combinations until the top 'n by
///       n' submatrix is the identity matrix.
/// - Then for any user provided vector 'd' of 'n' data words, we compute
///
///    A d = [ d |
///          [ c ]
///
///   where 'c' are the 'm' new parity code words.
///
/// All arithmetic is performed in GF(2^8) modulo a prime polynomial of order 8.
pub struct VandermondReedSolomonEncoder {
    field: GaloisField8Bit,

    /// 'n + m' by 'n'
    encoder: GaloisField8BitMatrix,
}

impl VandermondReedSolomonEncoder {
    pub const STANDARD_POLY: u8 = 0x1D;

    pub fn new(n: usize, m: usize, primitive_poly: u8) -> Self {
        assert!(256 >= n + m);

        let field = GaloisField8Bit::new(primitive_poly);

        let mut encoder = GaloisField8BitMatrix::zero(n + m, n);
        for i in 0..(n + m) {
            for j in 0..n {
                encoder[(i, j)] = field.pow(i as u8, j as u8);
            }
        }

        // Make the first 'n' rows of the matrix the identity matrix.
        // TODO: Deduplicate with gaussian elimination code.
        // This is essentially Gaussian elimination instead using column operations to
        // get a column reduced echelon form.
        for i in 0..n {
            // Swap columns until (i, i) is non-zero.
            {
                let mut j = i;
                while encoder[(i, j)] == 0 {
                    j += 1;
                }
                encoder.swap_cols(i, j);
                assert_ne!(encoder[(i, i)], 0);
            }

            // Scale column 'i' so that (i, i) is 1.
            {
                let s = field.div(1, encoder[(i, i)]);
                for j in 0..encoder.cols() {
                    encoder[(i, j)] = field.mul(encoder[(i, j)], s);
                }
                assert_eq!(encoder[(i, i)], 1);
            }

            // Subtract a multiple of column 'i' from all other columns so that they contain
            // a zero in row 'i'.
            for j in 0..encoder.cols() {
                if i == j {
                    continue;
                }

                let s = encoder[(i, j)];
                if s == 0 {
                    continue;
                }

                // encoder[:, j] -= encoder[i, j] * encoder[:, i]
                for k in 0..encoder.rows() {
                    encoder[(k, j)] = field.sub(encoder[(k, j)], field.mul(s, encoder[(k, i)]));
                }

                assert_eq!(encoder[(i, j)], 0);
            }
        }

        // Verify we correctly converted the top part of the matrix to the identity
        // matrix.
        for i in 0..n {
            for j in 0..n {
                assert_eq!(encoder[(i, j)], if i == j { 1 } else { 0 });
            }
        }

        Self { field, encoder }
    }

    /// Computes one of the 'm' parity words given the original 'n' data words.
    pub fn compute_parity_word(&self, data: &[u8], index: usize) -> u8 {
        let n = self.encoder.cols();
        assert_eq!(data.len(), n);

        // Multiply column 'n + index' of self.encoder with the data (a column vector).
        let mut sum = 0;
        for i in 0..data.len() {
            sum = self
                .field
                .add(sum, self.field.mul(data[i], self.encoder[(n + index, i)]));
        }

        sum
    }

    pub fn create_decoder(&self, parts: &[usize]) -> VandermondReedSolomonDecoder {
        let n = self.encoder.cols();
        assert_eq!(parts.len(), n);

        // Make an 'n x 2n' matrix from all parts (right side is identity).
        let mut mat = GaloisField8BitMatrix::zero(n, 2 * n);
        for i in 0..n {
            for j in 0..self.encoder.cols() {
                mat[(i, j)] = self.encoder[(parts[i], j)];
            }

            mat[(i, n + i)] = 1;
        }

        let mut original = mat.clone();

        // Invert the left half into the right half.
        mat.gaussian_elimination(&self.field);

        // Verify gaussian elimination was successful.
        {
            for i in 0..n {
                for j in 0..n {
                    let v = if i == j { 1 } else { 0 };
                    assert_eq!(mat[(i, j)], v);
                }
            }
        }

        let mut decoder = GaloisField8BitMatrix::zero(n, n);
        for i in 0..n {
            for j in 0..n {
                decoder[(i, j)] = mat[(i, n + j)];
            }
        }

        VandermondReedSolomonDecoder {
            decoder,
            field: &self.field,
        }
    }
}

pub struct VandermondReedSolomonDecoder<'a> {
    /// 'n' by 'n'
    decoder: GaloisField8BitMatrix,

    field: &'a GaloisField8Bit,
}

impl<'a> VandermondReedSolomonDecoder<'a> {
    pub fn compute_data_word(&self, part_words: &[u8], index: usize) -> u8 {
        let n = self.decoder.cols();
        assert_eq!(part_words.len(), n);

        let mut sum = 0;
        for i in 0..part_words.len() {
            sum = self
                .field
                .add(sum, self.field.mul(part_words[i], self.decoder[(index, i)]));
        }

        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Iterates over every distinct combination of K subset elements out of a N
    /// size set. On each iteration, K indexes are returned where each index is
    /// < N.
    ///
    /// TODO: Move to a shared library.
    pub struct CombinationIndexIter {
        index: Vec<usize>,
        subset_size: usize,
        set_size: usize,
    }

    impl CombinationIndexIter {
        pub fn new(subset_size: usize, set_size: usize) -> Self {
            assert!(subset_size <= set_size);

            Self {
                index: vec![],
                subset_size,
                set_size,
            }
        }

        pub fn next(&mut self) -> Option<&[usize]> {
            if self.index.len() == 0 {
                if self.subset_size == 0 {
                    return None;
                }

                self.index.reserve_exact(self.subset_size);
                for i in 0..self.subset_size {
                    self.index.push(i);
                }

                return Some(&self.index);
            }

            for i in (0..self.index.len()).rev() {
                let after = self.index.len() - i;
                if self.index[i] + 1 + after < self.set_size {
                    self.index[i] += 1;
                    for j in (i + 1)..self.index.len() {
                        self.index[j] = self.index[j - 1] + 1;
                    }
                    return Some(&self.index);
                }
            }

            None
        }
    }

    #[test]
    fn create_encoder() {
        let data: &'static [&'static [u8; 9]] = &[
            &[0xFF; 9],
            &[0x00; 9],
            &[1, 2, 3, 4, 5, 6, 7, 8, 9],
            &[0xFA, 0xBF, 0x11, 0, 0x02, 0x55, 0xDD, 0x20, 7],
        ];

        let n_m = &[(9, 3), (8, 6), (4, 2), (3, 1), (2, 0), (1, 0)];

        for (n, m) in n_m.iter().cloned() {
            let enc = VandermondReedSolomonEncoder::new(
                n,
                m,
                VandermondReedSolomonEncoder::STANDARD_POLY,
            );

            for data in data {
                let data = &data[0..n];

                let mut parity = vec![];
                for i in 0..m {
                    parity.push(enc.compute_parity_word(data, i));
                }

                let mut iter = CombinationIndexIter::new(n, n + m);
                while let Some(parts) = iter.next() {
                    let dec = enc.create_decoder(parts);

                    let mut words = vec![];
                    for i in parts.iter().cloned() {
                        if i < n {
                            words.push(data[i]);
                        } else {
                            words.push(parity[i - n])
                        }
                    }

                    let mut reconstructed_data = vec![];
                    for i in 0..n {
                        reconstructed_data.push(dec.compute_data_word(&words, i));
                    }

                    assert_eq!(&reconstructed_data, data);
                }
            }
        }
    }
}
