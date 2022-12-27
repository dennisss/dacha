use alloc::boxed::Box;
use std::vec::Vec;

use crate::hasher::Hasher;
use crate::sha512::SHA512Hasher;

const INITIAL_HASH: [u64; 8] = [
    0xcbbb9d5dc1059ed8,
    0x629a292a367cd507,
    0x9159015a3070dd17,
    0x152fecd8f70e5939,
    0x67332667ffc00b31,
    0x8eb44a8768581511,
    0xdb0c2e0d64f98fa7,
    0x47b5481dbefa4fa4,
];

const OUTPUT_SIZE: usize = 384 / 8;

#[derive(Clone)]
pub struct SHA384Hasher {
    inner: SHA512Hasher,
}

impl Default for SHA384Hasher {
    fn default() -> Self {
        Self {
            inner: SHA512Hasher::new_with_hash(&INITIAL_HASH),
        }
    }
}

impl Hasher for SHA384Hasher {
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

    #[test]
    fn sha384_test() {
        let h = |s: &str| {
            let mut hasher = SHA384Hasher::default();
            hasher.update(s.as_bytes());
            hasher.finish()
        };

        assert_eq!(&h("")[..],
				&hex!("38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b")[..]);
        assert_eq!(&h("The quick brown fox jumps over the lazy dog")[..],
				&hex!("ca737f1014a48f4c0b6dd43cb177b0afd9e5169367544c494011e3317dbf9a509cb1e5dc1e85a941bbee3d7f2afbc9b1")[..])

        // TODO: Test partial updates
    }
}
