use alloc::vec::Vec;

use crate::cipher::BlockCipher;
use crate::utils::xor;

macro_rules! next_block {
    ($data:ident, $size:expr) => {{
        let (block, rest) = $data.split_at($size);
        $data = rest;
        block
    }};
}

macro_rules! next_block_mut {
    ($data:ident, $size:expr) => {{
        let (block, rest) = $data.split_at_mut($size);
        $data = rest;
        block
    }};
}

pub struct ECBModeCipher<C: BlockCipher> {
    cipher: C,
}

impl<C: BlockCipher> ECBModeCipher<C> {
    pub fn new(cipher: C) -> Self {
        Self { cipher }
    }

    pub fn encrypt(&self, mut input: &[u8], mut output: &mut [u8]) {
        assert_eq!(input.len() % self.cipher.block_size(), 0);
        assert_eq!(input.len(), output.len());

        let block_size = self.cipher.block_size();
        let nblocks = input.len() / block_size;

        for _ in 0..nblocks {
            let input_block = next_block!(input, block_size);
            let output_block = next_block_mut!(output, block_size);

            self.cipher.encrypt_block(input_block, output_block);
        }
    }

    pub fn decrypt(&self, mut input: &[u8], mut output: &mut [u8]) {
        assert_eq!(input.len() % self.cipher.block_size(), 0);
        assert_eq!(input.len(), output.len());

        let block_size = self.cipher.block_size();
        let nblocks = input.len() / block_size;

        for _ in 0..nblocks {
            let input_block = next_block!(input, block_size);
            let output_block = next_block_mut!(output, block_size);

            self.cipher.decrypt_block(input_block, output_block);
        }
    }
}

// TODO: Start testing this.
pub struct CBCModeCipher<C: BlockCipher> {
    cipher: C,
    iv: Vec<u8>,
}

impl<C: BlockCipher> CBCModeCipher<C> {
    pub fn encrypt(&mut self, mut input: &[u8], mut output: &mut [u8]) {
        assert_eq!(input.len() % self.cipher.block_size(), 0);
        assert_eq!(input.len(), output.len());

        let block_size = self.cipher.block_size();
        let nblocks = input.len() / block_size;

        // Intermediate buffer for storing the result of the xor
        let mut buf = vec![0; block_size];

        let mut iv: &[u8] = &self.iv;

        for _ in 0..nblocks {
            let input_block = next_block!(input, block_size);
            let output_block = next_block_mut!(output, block_size);

            xor(iv, input_block, &mut buf);
            self.cipher.encrypt_block(&buf, output_block);
            iv = output_block;
        }
    }

    pub fn decrypt(&mut self, mut input: &[u8], mut output: &mut [u8]) {
        assert_eq!(input.len() % self.cipher.block_size(), 0);
        assert_eq!(input.len(), output.len());

        let block_size = self.cipher.block_size();
        let nblocks = input.len() / block_size;

        let mut buf = vec![0; block_size];

        let mut iv: &[u8] = &self.iv;

        for _ in 0..nblocks {
            let input_block = next_block!(input, block_size);
            let output_block = next_block_mut!(output, block_size);

            self.cipher.decrypt_block(input_block, &mut buf);
            xor(&buf, iv, output_block);
            iv = input_block;
        }
    }
}
