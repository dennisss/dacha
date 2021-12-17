// Software implementation of AES-CCM built on top of the ECB peripheral on the
// NRF chips.
//
// See https://datatracker.ietf.org/doc/html/rfc3610

use core::result::Result;

use crypto::ccm::BlockCipherBuffer;

use crate::ecb::*;

/// Number of bytes in an AES-128 key.
pub const KEY_SIZE: usize = 16;

/// Number of bytes used to represent the message length.
const LENGTH_SIZE: usize = 2;

/// Number of bytes used to store the message authentication tag.
const TAG_SIZE: usize = 4;

const NONCE_SIZE: usize = 15 - LENGTH_SIZE;

const BLOCK_SIZE: usize = 16; // 128-bit blocks

/*
Encrpytions to do:
1. CBC Base flags block
2. CBC ADD data block (should fit in one )
3. Encrypt S_0 for MIC
4. Encrypt first plaintext block
5. Encrypt second plaintext block
*/

pub struct AES128BlockBuffer<'a> {
    ecb: &'a mut ECB,
    data: ECBData,
}

impl<'a> AES128BlockBuffer<'a> {
    pub fn new(key: &[u8; KEY_SIZE], ecb: &'a mut ECB) -> Self {
        Self {
            ecb,
            data: ECBData {
                key: key.clone(),
                // TODO: Only need to append the nonce in the middle.
                plaintext: [0u8; BLOCK_SIZE],
                ciphertext: [0u8; BLOCK_SIZE],
            },
        }
    }
}

impl<'a> BlockCipherBuffer for AES128BlockBuffer<'a> {
    fn plaintext(&self) -> &[u8; BLOCK_SIZE] {
        &self.data.plaintext
    }

    fn plaintext_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
        &mut self.data.plaintext
    }

    fn plaintext_mut_ciphertext(&mut self) -> (&mut [u8; BLOCK_SIZE], &[u8; BLOCK_SIZE]) {
        (&mut self.data.plaintext, &self.data.ciphertext)
    }

    fn encrypt(&mut self) {
        self.ecb.encrypt(&mut self.data);
    }

    fn ciphertext(&self) -> &[u8; BLOCK_SIZE] {
        &self.data.ciphertext
    }
}
