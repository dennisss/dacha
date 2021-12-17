use alloc::boxed::Box;
use std::vec::Vec;

use crate::hasher::Hasher;
use crate::sha256::SHA256Hasher;

const INITIAL_HASH: [u32; 8] = [
    0xc1059ed8, 0x367cd507, 0x3070dd17, 0xf70e5939, 0xffc00b31, 0x68581511, 0x64f98fa7, 0xbefa4fa4,
];

const OUTPUT_SIZE: usize = 28;

#[derive(Clone)]
pub struct SHA224Hasher {
    inner: SHA256Hasher,
}

impl Default for SHA224Hasher {
    fn default() -> Self {
        Self {
            inner: SHA256Hasher::new_with_hash(&INITIAL_HASH),
        }
    }
}

impl Hasher for SHA224Hasher {
    fn block_size(&self) -> usize {
        self.inner.block_size()
    }

    fn output_size(&self) -> usize {
        OUTPUT_SIZE
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    fn finish(&self) -> Vec<u8> {
        let mut hash = self.inner.finish();
        hash.truncate(self.output_size());
        hash
    }

    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::hex;

    #[test]
    fn sha224_test() {
        let h = |s: &str| {
            let mut hasher = SHA224Hasher::default();
            hasher.update(s.as_bytes());
            hasher.finish()
        };

        assert_eq!(
            &h("")[..],
            &hex::decode("d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f").unwrap()[..]
        );
        assert_eq!(
            &h("The quick brown fox jumps over the lazy dog")[..],
            &hex::decode("730e109bd7a8a32b1cb9d9a09aa2325d2430587ddbc0c38bad911525").unwrap()[..]
        )

        // TODO: Test partial updates
    }
}
