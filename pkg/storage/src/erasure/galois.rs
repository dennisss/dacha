use alloc::vec::Vec;
use core::fmt::Debug;
use core::ops::{Index, IndexMut};

use math::argmax::argmax;

const FIELD_SIZE: usize = 1 << 8; // 2^8

/// Operations on scalar values in the field GF(2^8)
pub struct GaloisField8Bit {
    /// Lower 8 coefficients of the 8th order polynomial used for reducing
    /// numbers that exceed 2^8-1.
    poly: u8,

    /// For an index 'i' which is an element of GF(2^8), value[i] is '2^i'
    /// exp2[255] is undefined
    exp2: [u8; FIELD_SIZE],

    /// For an index '2^i', 'value[2^i] = i'
    /// log2[0] is undefined.
    log2: [u8; FIELD_SIZE],
}

impl GaloisField8Bit {
    pub fn new(poly: u8) -> Self {
        let mut exp2 = [0u8; FIELD_SIZE];
        let mut log2 = [0u8; FIELD_SIZE];

        // Pre-compute all powers of 2.
        let mut v = 1;
        for i in 0..(FIELD_SIZE - 1) {
            exp2[i] = v;
            log2[v as usize] = i as u8;

            assert!(v != 0);

            // Multiply by 2.
            let overflow = v & (1 << 7) != 0;
            v <<= 1;
            if overflow {
                v ^= poly;
            }
        }

        for i in 0..(exp2.len() - 1) {
            for j in (i + 1)..(exp2.len() - 1) {
                assert_ne!(exp2[i], exp2[j]);
            }
        }

        Self { poly, exp2, log2 }
    }

    pub fn add(&self, a: u8, b: u8) -> u8 {
        a ^ b
    }

    pub fn sub(&self, a: u8, b: u8) -> u8 {
        self.add(a, b)
    }

    /// a^b = (2^i)^b
    pub fn pow(&self, a: u8, b: u8) -> u8 {
        if b == 0 {
            return 1;
        }

        if a == 0 {
            return 0;
        }

        let i = self.log2[a as usize] as u16;
        let k = (i * (b as u16)) % ((FIELD_SIZE - 1) as u16);

        self.exp2[k as usize]
    }

    /// a * b = 2^i * 2^j
    ///       = 2^(i + j)
    pub fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            return 0;
        }

        let i = self.log2[a as usize] as usize;
        let j = self.log2[b as usize] as usize;

        let mut k = i + j;
        if k >= (FIELD_SIZE - 1) {
            k -= (FIELD_SIZE - 1);
        }

        assert!(k != 255);
        self.exp2[k]
    }

    /// a / b = 2^i / 2^j
    ///       = 2^(i - j)
    pub fn div(&self, a: u8, b: u8) -> u8 {
        assert_ne!(b, 0);
        if a == 0 {
            return 0;
        }

        let i = self.log2[a as usize] as usize;
        let j = self.log2[b as usize] as usize;

        let mut k = i + (FIELD_SIZE - 1) - j;
        if k >= (FIELD_SIZE - 1) {
            k -= (FIELD_SIZE - 1);
        }

        assert!(k != 255);
        self.exp2[k]
    }
}

/// 2D matrix composed of elements in GF(2^8).
#[derive(Clone)]
pub struct GaloisField8BitMatrix {
    values: Vec<u8>,
    rows: usize,
    cols: usize,
}

impl Debug for GaloisField8BitMatrix {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for i in 0..self.rows {
            for j in 0..self.cols {
                write!(f, "{}, ", self[(i, j)]);
            }

            write!(f, "\n");
        }

        Ok(())
    }
}

impl Index<(usize, usize)> for GaloisField8BitMatrix {
    type Output = u8;

    fn index(&self, index: (usize, usize)) -> &Self::Output {
        assert!(index.1 < self.cols);
        &self.values[index.0 * self.cols + index.1]
    }
}

impl IndexMut<(usize, usize)> for GaloisField8BitMatrix {
    fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
        assert!(index.1 < self.cols);
        &mut self.values[index.0 * self.cols + index.1]
    }
}

impl GaloisField8BitMatrix {
    pub fn zero(rows: usize, cols: usize) -> Self {
        Self {
            values: vec![0; rows * cols],
            rows,
            cols,
        }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn swap_rows(&mut self, i: usize, j: usize) {
        if i == j {
            return;
        }

        for k in 0..self.cols {
            let tmp = self[(i, k)];
            self[(i, k)] = self[(j, k)];
            self[(j, k)] = tmp;
        }
    }

    pub fn swap_cols(&mut self, i: usize, j: usize) {
        if i == j {
            return;
        }

        for k in 0..self.rows {
            let tmp = self[(k, i)];
            self[(k, i)] = self[(k, j)];
            self[(k, j)] = tmp;
        }
    }

    /// TODO: Deduplicate with math::Matrix.
    pub fn gaussian_elimination(&mut self, field: &GaloisField8Bit) {
        let mut h = 0; // Current pivot row.
        let mut k = 0; // Current pivot column.

        while h < self.rows() && k < self.cols() {
            // Find row index with highest value in the current column.
            // TODO: Can pick any non-zero one as we don't have numerical precision issues
            // with integers.
            let mut i_max = argmax(h..self.rows(), |i| self[(i, k)]).unwrap();

            if self[(i_max, k)] == 0 {
                // This column has no pivot.
                k += 1
            } else {
                self.swap_rows(h, i_max);

                // Normalize the pivot row.
                let s = field.div(1, self[(h, k)]);
                for j in h..self.cols() {
                    self[(h, j)] = field.mul(self[(h, j)], s);
                }

                // Use (h+1)..self.rows() if you don't need the upper right to be
                // reduced
                for i in 0..self.rows() {
                    // assert_eq!(h, k);
                    if i == h {
                        continue;
                    }

                    if self[(i, k)] == 0 {
                        continue;
                    }

                    let f = field.div(self[(i, k)], self[(h, k)]);
                    self[(i, k)] = 0;
                    for j in (k + 1)..self.cols() {
                        self[(i, j)] = field.sub(self[(i, j)], field.mul(f, self[(h, j)]));
                    }
                }

                h += 1;
                k += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_arithmetic() {
        let field = GaloisField8Bit::new(0x1D);

        assert_eq!(field.mul(2, 3), 6);
        assert_eq!(field.mul(4, 3), 12);
        assert_eq!(field.mul(5, 3), 12 ^ 3);
        assert_eq!(field.mul(200, 1), 200);
    }

    #[test]
    fn all_numbers_have_inverses() {
        let field = GaloisField8Bit::new(0x1D);

        for i in 1..=255 {
            let inv = field.div(1, i);
            if i != 1 {
                assert_ne!(i, inv);
            }
            assert_eq!(1, field.mul(i, inv));
        }
    }
}
