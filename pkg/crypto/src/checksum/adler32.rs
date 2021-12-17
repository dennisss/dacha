use alloc::boxed::Box;
use std::vec::Vec;

use crate::hasher::*;

const ADLER32_PRIME_MOD: usize = 65521;

#[derive(Clone)]
pub struct Adler32Hasher {
    // NOTE: Must be >16bits each
    s1: usize,
    s2: usize,
}

impl Adler32Hasher {
    pub fn new() -> Self {
        Self::from_hash(1)
    }

    pub fn from_hash(hash: u32) -> Self {
        Self {
            s1: (hash & 0xffff) as usize,
            s2: ((hash >> 16) & 0xffff) as usize,
        }
    }

    pub fn finish_u32(&self) -> u32 {
        ((self.s2 << 16) | self.s1) as u32
    }
}

impl Hasher for Adler32Hasher {
    fn block_size(&self) -> usize {
        1
    }

    fn output_size(&self) -> usize {
        4
    }

    fn update(&mut self, data: &[u8]) {
        for v in data.iter().cloned() {
            self.s1 = (self.s1 + (v as usize)) % ADLER32_PRIME_MOD;
            self.s2 = (self.s2 + self.s1) % ADLER32_PRIME_MOD;
        }
    }

    fn finish(&self) -> Vec<u8> {
        self.finish_u32().to_be_bytes().to_vec()
    }

    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}
