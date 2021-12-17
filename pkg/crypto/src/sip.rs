use alloc::boxed::Box;
use std::num::Wrapping;
use std::vec::Vec;

use crate::hasher::Hasher;

/// Based on https://www.aumasson.jp/siphash/siphash.pdf.
#[derive(Clone)]
pub struct SipHasher {
    num_compression_rounds: usize,
    num_finalization_rounds: usize,

    v0: Wrapping<u64>,
    v1: Wrapping<u64>,
    v2: Wrapping<u64>,
    v3: Wrapping<u64>,

    /// Total number of bytes read mod 256.
    bytes_processed: Wrapping<u8>,

    /// Current word being worked on. May be only partially full.
    word: [u8; 8],

    /// Number of bytes in word which are full of valid bytes.
    word_size: usize,
}

impl SipHasher {
    pub fn new(num_compression_rounds: usize, num_finalization_rounds: usize, key: &[u8]) -> Self {
        assert_eq!(key.len(), 16);

        Self::new_with_key_halves(
            num_compression_rounds,
            num_finalization_rounds,
            u64::from_le_bytes(*array_ref![key, 0, 8]),
            u64::from_le_bytes(*array_ref![key, 8, 8]),
        )
    }

    pub fn new_with_key_halves(
        num_compression_rounds: usize,
        num_finalization_rounds: usize,
        key_0: u64,
        key_1: u64,
    ) -> Self {
        Self {
            num_compression_rounds,
            num_finalization_rounds,

            v0: Wrapping(key_0 ^ 0x736f6d6570736575),
            v1: Wrapping(key_1 ^ 0x646f72616e646f6d),
            v2: Wrapping(key_0 ^ 0x6c7967656e657261),
            v3: Wrapping(key_1 ^ 0x7465646279746573),

            bytes_processed: Wrapping(0),

            word: [0u8; 8],
            word_size: 0,
        }
    }

    pub fn default_rounds_with_key_halves(key_0: u64, key_1: u64) -> Self {
        Self::new_with_key_halves(2, 4, key_0, key_1)
    }

    /// Implementation of the 'SipRound' function.
    fn run_round(&mut self) {
        self.v0 += self.v1;
        self.v2 += self.v3;

        self.v1 = self.v1.rotate_left(13);
        self.v3 = self.v3.rotate_left(16);

        self.v1 ^= self.v0;
        self.v3 ^= self.v2;

        self.v0 = self.v0.rotate_left(32);

        self.v2 += self.v1;
        self.v0 += self.v3;

        self.v1 = self.v1.rotate_left(17);
        self.v3 = self.v3.rotate_left(21);

        self.v1 ^= self.v2;
        self.v3 ^= self.v0;

        self.v2 = self.v2.rotate_left(32);
    }

    fn apply_word(&mut self) {
        let w = Wrapping(u64::from_le_bytes(self.word));
        self.v3 ^= w;

        for _ in 0..self.num_compression_rounds {
            self.run_round();
        }

        self.v0 ^= w;
    }

    pub fn finish_u64(mut self) -> u64 {
        // Pad up to 7 bytes
        // (NOTE: word_size will never be 8 here)
        for i in self.word_size..(self.word.len() - 1) {
            self.word[i] = 0;
        }

        // Add final length byte.
        self.word[7] = self.bytes_processed.0;

        // Process final word.
        self.apply_word();

        // Finalization
        self.v2 ^= Wrapping(0xff);

        for _ in 0..self.num_finalization_rounds {
            self.run_round();
        }

        (self.v0 ^ self.v1 ^ self.v2 ^ self.v3).0
    }
}

impl Hasher for SipHasher {
    fn block_size(&self) -> usize {
        8
    }
    fn output_size(&self) -> usize {
        8
    }

    fn update(&mut self, mut data: &[u8]) {
        while !data.is_empty() {
            // Attempt to fill up the current word.
            let n = std::cmp::min(data.len(), self.word.len() - self.word_size);
            self.word[self.word_size..(self.word_size + n)].copy_from_slice(&data[0..n]);
            self.word_size += n;
            data = &data[n..];

            self.bytes_processed += Wrapping(n as u8);

            if self.word_size == self.word.len() {
                self.apply_word();
                self.word_size = 0;
            }
        }
    }

    fn finish(&self) -> Vec<u8> {
        self.clone().finish_u64().to_be_bytes().to_vec()
    }

    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sip_test() {
        let mut h = SipHasher::new(
            2,
            4,
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        );

        h.update(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);

        assert_eq!(h.finish_u64(), 0xa129ca6149be45e5);

        // TODO: Add more test vectors.
    }
}
