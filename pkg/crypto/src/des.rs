/*
64bit key (56 used + 8 parity)

Using the wikipedia standard of https://en.wikipedia.org/wiki/DES_supplementary_material
- All ordered in big-endian.
- First bit is most significant bit of the most significant byte

When using a u64, we assume that it was created from big endian bytes
*/

// TODO: Check for weak keys and check for parity

use std::vec::Vec;

use crate::cipher::BlockCipher;
use common::bits::BitVector;

type DESKey = [u8; 8];

type DESBlock = [u8; 8];

const NUM_ROUNDS: usize = 16;

const INITIAL_PERMUTATION: [u8; 64] = [
    57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61, 53, 45, 37, 29, 21, 13, 5, 63,
    55, 47, 39, 31, 23, 15, 7, 56, 48, 40, 32, 24, 16, 8, 0, 58, 50, 42, 34, 26, 18, 10, 2, 60, 52,
    44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
];

const EXPANSION_PERMUTATION: [u8; 48] = [
    31, 0, 1, 2, 3, 4, 3, 4, 5, 6, 7, 8, 7, 8, 9, 10, 11, 12, 11, 12, 13, 14, 15, 16, 15, 16, 17,
    18, 19, 20, 19, 20, 21, 22, 23, 24, 23, 24, 25, 26, 27, 28, 27, 28, 29, 30, 31, 0,
];

// Each box maps a 6-bit value to a 4-bit value
const S_BOXES: [[u8; 64]; 8] = [
    [
        14, 0, 4, 15, 13, 7, 1, 4, 2, 14, 15, 2, 11, 13, 8, 1, 3, 10, 10, 6, 6, 12, 12, 11, 5, 9,
        9, 5, 0, 3, 7, 8, 4, 15, 1, 12, 14, 8, 8, 2, 13, 4, 6, 9, 2, 1, 11, 7, 15, 5, 12, 11, 9, 3,
        7, 14, 3, 10, 10, 0, 5, 6, 0, 13,
    ],
    [
        15, 3, 1, 13, 8, 4, 14, 7, 6, 15, 11, 2, 3, 8, 4, 14, 9, 12, 7, 0, 2, 1, 13, 10, 12, 6, 0,
        9, 5, 11, 10, 5, 0, 13, 14, 8, 7, 10, 11, 1, 10, 3, 4, 15, 13, 4, 1, 2, 5, 11, 8, 6, 12, 7,
        6, 12, 9, 0, 3, 5, 2, 14, 15, 9,
    ],
    [
        10, 13, 0, 7, 9, 0, 14, 9, 6, 3, 3, 4, 15, 6, 5, 10, 1, 2, 13, 8, 12, 5, 7, 14, 11, 12, 4,
        11, 2, 15, 8, 1, 13, 1, 6, 10, 4, 13, 9, 0, 8, 6, 15, 9, 3, 8, 0, 7, 11, 4, 1, 15, 2, 14,
        12, 3, 5, 11, 10, 5, 14, 2, 7, 12,
    ],
    [
        7, 13, 13, 8, 14, 11, 3, 5, 0, 6, 6, 15, 9, 0, 10, 3, 1, 4, 2, 7, 8, 2, 5, 12, 11, 1, 12,
        10, 4, 14, 15, 9, 10, 3, 6, 15, 9, 0, 0, 6, 12, 10, 11, 1, 7, 13, 13, 8, 15, 9, 1, 4, 3, 5,
        14, 11, 5, 12, 2, 7, 8, 2, 4, 14,
    ],
    [
        2, 14, 12, 11, 4, 2, 1, 12, 7, 4, 10, 7, 11, 13, 6, 1, 8, 5, 5, 0, 3, 15, 15, 10, 13, 3, 0,
        9, 14, 8, 9, 6, 4, 11, 2, 8, 1, 12, 11, 7, 10, 1, 13, 14, 7, 2, 8, 13, 15, 6, 9, 15, 12, 0,
        5, 9, 6, 10, 3, 4, 0, 5, 14, 3,
    ],
    [
        12, 10, 1, 15, 10, 4, 15, 2, 9, 7, 2, 12, 6, 9, 8, 5, 0, 6, 13, 1, 3, 13, 4, 14, 14, 0, 7,
        11, 5, 3, 11, 8, 9, 4, 14, 3, 15, 2, 5, 12, 2, 9, 8, 5, 12, 15, 3, 10, 7, 11, 0, 14, 4, 1,
        10, 7, 1, 6, 13, 0, 11, 8, 6, 13,
    ],
    [
        4, 13, 11, 0, 2, 11, 14, 7, 15, 4, 0, 9, 8, 1, 13, 10, 3, 14, 12, 3, 9, 5, 7, 12, 5, 2, 10,
        15, 6, 8, 1, 6, 1, 6, 4, 11, 11, 13, 13, 8, 12, 1, 3, 4, 7, 10, 14, 7, 10, 9, 15, 5, 6, 0,
        8, 15, 0, 14, 5, 2, 9, 3, 2, 12,
    ],
    [
        13, 1, 2, 15, 8, 13, 4, 8, 6, 10, 15, 3, 11, 7, 1, 4, 10, 12, 9, 5, 3, 6, 14, 11, 5, 0, 0,
        14, 12, 9, 7, 2, 7, 2, 11, 1, 4, 14, 1, 7, 9, 4, 12, 10, 14, 8, 2, 13, 0, 15, 6, 12, 10, 9,
        13, 0, 15, 3, 3, 5, 5, 6, 8, 11,
    ],
];

const P_BOX: [u8; 32] = [
    15, 6, 19, 20, 28, 11, 27, 16, 0, 14, 22, 25, 4, 17, 30, 9, 1, 7, 23, 13, 31, 26, 2, 8, 18, 12,
    29, 5, 21, 10, 3, 24,
];

// Key schedule permutations.

const PC_1: [u8; 56] = [
    56, 48, 40, 32, 24, 16, 8, 0, 57, 49, 41, 33, 25, 17, 9, 1, 58, 50, 42, 34, 26, 18, 10, 2, 59,
    51, 43, 35, 62, 54, 46, 38, 30, 22, 14, 6, 61, 53, 45, 37, 29, 21, 13, 5, 60, 52, 44, 36, 28,
    20, 12, 4, 27, 19, 11, 3,
];

const PC_2: [u8; 48] = [
    13, 16, 10, 23, 0, 4, 2, 27, 14, 5, 20, 9, 22, 18, 11, 3, 25, 7, 15, 6, 26, 19, 12, 1, 40, 51,
    30, 36, 46, 54, 29, 39, 50, 44, 32, 47, 43, 48, 38, 55, 33, 52, 45, 41, 49, 35, 28, 31,
];

// During each round, by how many bits we should rotate left
// before generating the sub key.
const ROTATION_SCHEDULE: [u8; NUM_ROUNDS] = [1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 2, 2, 2, 1];

#[derive(Clone)]
pub struct DESBlockCipher {
    // TODO: Make static length.
    round_keys: Vec<BitVector>,
}

impl DESBlockCipher {
    pub fn new(key: &[u8]) -> Self {
        assert_eq!(key.len(), 8);

        let mut round_keys = vec![];

        let key_vec = BitVector::from(key, 8 * 8);

        let mut key_schedule = DESKeySchedule::new(&key_vec);
        for _ in 0..NUM_ROUNDS {
            round_keys.push(key_schedule.next_key());
        }

        Self { round_keys }
    }

    fn crypt_block(&self, block: &[u8], out: &mut [u8], encrypting: bool) {
        assert_eq!(block.len(), self.block_size());
        assert_eq!(out.len(), self.block_size());

        let mut cipher = BitVector::from(block, 8 * block.len());

        cipher = Self::initial_permutation(&cipher);

        {
            let (mut cipher_left, mut cipher_right) = cipher.split_at(32);
            for i in 0..NUM_ROUNDS {
                let round_key = &self.round_keys[if encrypting { i } else { NUM_ROUNDS - i - 1 }];

                cipher_left = cipher_left.xor(&Self::feistel_function(&cipher_right, round_key));

                // Swap halves.
                if i != (NUM_ROUNDS - 1) {
                    tup!((cipher_left, cipher_right) = (cipher_right, cipher_left));
                }
            }

            cipher = cipher_left.concat(&cipher_right);
        }

        cipher = Self::final_permutation(&cipher);

        // Copy result into output.
        out.copy_from_slice(cipher.as_ref());
    }

    // Input and output are 64bit vectors.
    fn initial_permutation(input: &BitVector) -> BitVector {
        input.permute(&INITIAL_PERMUTATION)
    }

    // Input and output are 64bit vectors.
    pub fn final_permutation(input: &BitVector) -> BitVector {
        let mut out = input.clone();
        for i in 0..INITIAL_PERMUTATION.len() {
            let j = INITIAL_PERMUTATION[i];
            assert!(out.set(j as usize, input.get(i).unwrap()));
        }

        out
    }

    // half_block: 32bits
    // sub_key: 48bits from key schedule
    //
    // returns 32-bits
    fn feistel_function(half_block: &BitVector, sub_key: &BitVector) -> BitVector {
        // Expand to 48-bits
        let expanded_block = half_block.permute(&EXPANSION_PERMUTATION);

        // Combine key and half-block (still 48-bits)
        let mixed_block = expanded_block.xor(sub_key);

        // Apply s-boxes to reduce to 32-bits (6bits -> 4bits at a time)
        let mut substituted_block = BitVector::new();
        {
            let mut mixed_block_rest = mixed_block;
            for i in 0..8 {
                let (v, rest) = mixed_block_rest.split_at(6);
                mixed_block_rest = rest;

                let vs = S_BOXES[i][v.to_lower_msb()];
                substituted_block =
                    substituted_block.concat(&BitVector::from_lower_msb(vs as usize, 4));
            }
        }

        substituted_block.permute(&P_BOX)
    }
}

impl BlockCipher for DESBlockCipher {
    fn block_size(&self) -> usize {
        8
    }

    fn encrypt_block(&self, block: &[u8], out: &mut [u8]) {
        self.crypt_block(block, out, true);
    }

    fn decrypt_block(&self, block: &[u8], out: &mut [u8]) {
        self.crypt_block(block, out, false);
    }
}

pub struct DESKeySchedule {
    left_state: BitVector,
    right_state: BitVector,
    // Starts at 0 meaning that
    round: u8,
}

impl DESKeySchedule {
    // Input is 64bit key
    pub fn new(key: &BitVector) -> Self {
        assert_eq!(key.len(), 64);

        // PC-1 converts from 64-bit key to 2*28bit states.
        // (removing the padding bits)
        let full_state = key.permute(&PC_1);

        let (left_state, right_state) = full_state.split_at(28);

        assert_eq!(left_state.len(), 28);
        assert_eq!(right_state.len(), 28);

        Self {
            left_state,
            right_state,
            round: 0,
        }
    }

    // Each subkey is 48 bits
    pub fn next_key(&mut self) -> BitVector {
        let r = ROTATION_SCHEDULE[self.round as usize];
        self.round += 1;

        self.left_state = self.left_state.rotate_left(r as usize);
        self.right_state = self.right_state.rotate_left(r as usize);

        let full_state = self.left_state.concat(&self.right_state);

        let subkey = full_state.permute(&PC_2);
        subkey
    }
}

pub struct TripleDESBlockCipher {
    inner_ciphers: [DESBlockCipher; 3],
}

impl BlockCipher for TripleDESBlockCipher {
    fn block_size(&self) -> usize {
        self.inner_ciphers[0].block_size()
    }

    fn encrypt_block(&self, block: &[u8], out: &mut [u8]) {
        let mut buf = [0; 8];
        self.inner_ciphers[0].encrypt_block(block, out);
        self.inner_ciphers[1].decrypt_block(out, &mut buf);
        self.inner_ciphers[2].encrypt_block(&buf, out);
    }

    fn decrypt_block(&self, block: &[u8], out: &mut [u8]) {
        let mut buf = [0; 8];
        self.inner_ciphers[2].decrypt_block(block, out);
        self.inner_ciphers[1].encrypt_block(out, &mut buf);
        self.inner_ciphers[0].decrypt_block(&buf, out);
    }
}

impl TripleDESBlockCipher {
    // NOTE: key can be either 8, 16, or 24 bytes long.
    pub fn new(key: &[u8]) -> Self {
        let inner_ciphers = {
            if key.len() == 8 {
                let cipher = DESBlockCipher::new(key);
                [cipher.clone(), cipher.clone(), cipher.clone()]
            } else if key.len() == 16 {
                let c1 = DESBlockCipher::new(&key[0..8]);
                let c2 = DESBlockCipher::new(&key[8..]);
                [c1.clone(), c2, c1.clone()]
            } else if key.len() == 24 {
                let c1 = DESBlockCipher::new(&key[0..8]);
                let c2 = DESBlockCipher::new(&key[8..16]);
                let c3 = DESBlockCipher::new(&key[16..]);
                [c1, c2, c3]
            } else {
                panic!();
            }
        };

        Self { inner_ciphers }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn des_initial_final_permute() {
        // Verifiying the the IP and FP stages are inverses of each other.
        for plain in &openssl::des_ecb::PLAIN_DATA {
            let input = BitVector::from(plain, 64);
            let output =
                DESBlockCipher::final_permutation(&DESBlockCipher::initial_permutation(&input));
            assert_eq!(output.as_ref(), plain);
        }
    }

    #[test]
    fn des_encrypt() {
        /*
        var crypto = require('crypto');
        var c = crypto.createCipheriv('DES-ECB', Buffer.from('2321f2d0e092045c', 'hex'), null);
        var plain = Buffer.from('75130b9657220950', 'hex');
        console.log(c.update(plain).toString('hex'))

        eb98476e4418713b
        */
        {
            let key = hex!("2321f2d0e092045c");
            let plain = hex!("75130b9657220950");
            let cipher = hex!("eb98476e4418713b");

            let c = DESBlockCipher::new(&key);

            let mut output = vec![0u8; 8];
            c.encrypt_block(&plain, &mut output);

            assert_eq!(&cipher, &output[..]);
        }

        for i in 0..openssl::des_ecb::KEY_DATA.len() {
            let key: &[u8] = &openssl::des_ecb::KEY_DATA[i];
            let plain: &[u8] = &openssl::des_ecb::PLAIN_DATA[i];
            let cipher: &[u8] = &openssl::des_ecb::CIPHER_DATA[i];

            let c = DESBlockCipher::new(key);

            let mut output = vec![0u8; 8];
            c.encrypt_block(plain, &mut output);

            assert_eq!(cipher, &output);
        }
    }

    #[test]
    fn des_decrypt() {
        for i in 0..openssl::des_ecb::KEY_DATA.len() {
            let key: &[u8] = &openssl::des_ecb::KEY_DATA[i];
            let plain: &[u8] = &openssl::des_ecb::PLAIN_DATA[i];
            let cipher: &[u8] = &openssl::des_ecb::CIPHER_DATA[i];

            let c = DESBlockCipher::new(key);

            let mut output = vec![0u8; 8];
            c.decrypt_block(cipher, &mut output);

            assert_eq!(plain, &output);
        }
    }
}
