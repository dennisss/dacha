use alloc::boxed::Box;
use std::vec::Vec;

use crate::hasher::*;
use crate::md::*;
use generic_array::GenericArray;
use typenum::U128;

// TODO: Use https://en.wikipedia.org/wiki/Intel_SHA_extensions

const INITIAL_HASH: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

const INITIAL_HASH_224: [u64; 8] = [
    0x8C3D37C819544DA2,
    0x73E1996689DCD4D6,
    0x1DFAB7AE32FF9C82,
    0x679DD514582F9FCF,
    0x0F6D2B697BD44DA8,
    0x77E36F7304C48942,
    0x3F9D85A86A1D36C8,
    0x1112E6AD91D692A1,
];

const INITIAL_HASH_256: [u64; 8] = [
    0x22312194FC2BF72C,
    0x9F555FA3C84C64C2,
    0x2393B86B6F53B151,
    0x963877195940EABD,
    0x96283EE2A88EFFE3,
    0xBE5E1E2553863992,
    0x2B0199FC2C85B8AA,
    0x0EB72DDC81C52CA2,
];

const ROUND_CONSTANTS: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

type HashState = [u64; 8];

#[derive(Clone)]
pub struct SHA512Hasher {
    inner: MerkleDamgard<HashState, U128>, // 128 bytes is 1024 bits.
}

impl SHA512Hasher {
    pub fn new_with_hash(initial_hash: &HashState) -> Self {
        let padding = LengthPadding {
            big_endian: true,
            int128: true,
        };
        Self {
            inner: MerkleDamgard::new(*initial_hash, padding),
        }
    }

    fn update_chunk(chunk: &GenericArray<u8, U128>, hash: &mut HashState) {
        let mut w = [0u64; 80];
        for i in 0..16 {
            w[i] = u64::from_be_bytes(*array_ref![chunk, 8 * i, 8]);
        }
        for i in 16..80 {
            let s0 = w[i - 15].rotate_right(1) ^ w[i - 15].rotate_right(8) ^ (w[i - 15] >> 7);
            let s1 = w[i - 2].rotate_right(19) ^ w[i - 2].rotate_right(61) ^ (w[i - 2] >> 6);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let mut a = hash[0];
        let mut b = hash[1];
        let mut c = hash[2];
        let mut d = hash[3];
        let mut e = hash[4];
        let mut f = hash[5];
        let mut g = hash[6];
        let mut h = hash[7];

        for i in 0..80 {
            let S0 = a.rotate_right(28) ^ a.rotate_right(34) ^ a.rotate_right(39);
            let S1 = e.rotate_right(14) ^ e.rotate_right(18) ^ e.rotate_right(41);

            let ch = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(S1)
                .wrapping_add(ch)
                .wrapping_add(ROUND_CONSTANTS[i])
                .wrapping_add(w[i]);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = S0.wrapping_add(maj);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(h);
    }
}

impl Default for SHA512Hasher {
    fn default() -> Self {
        Self::new_with_hash(&INITIAL_HASH)
    }
}

impl Hasher for SHA512Hasher {
    fn block_size(&self) -> usize {
        128
    }

    fn output_size(&self) -> usize {
        64
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.update(data, Self::update_chunk);
    }

    fn finish(&self) -> Vec<u8> {
        let state = self.inner.finish(Self::update_chunk);

        // Generate final message by casting to big endian
        let mut hh = [0u8; 64];
        for i in 0..(64 / 8) {
            *array_mut_ref![hh, 8 * i, 8] = state[i].to_be_bytes();
        }

        hh.to_vec()
    }

    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}

/// SHA-512/224
#[derive(Clone)]
pub struct SHA512_224Hasher {
    inner: SHA512Hasher,
}

impl Default for SHA512_224Hasher {
    fn default() -> Self {
        Self {
            inner: SHA512Hasher::new_with_hash(&INITIAL_HASH_224),
        }
    }
}

impl Hasher for SHA512_224Hasher {
    fn block_size(&self) -> usize {
        self.inner.block_size()
    }

    fn output_size(&self) -> usize {
        28
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    fn finish(&self) -> Vec<u8> {
        let mut out = self.inner.finish();
        out.truncate(self.output_size());
        out
    }

    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}

/// SHA-512/256
#[derive(Clone)]
pub struct SHA512_256Hasher {
    inner: SHA512Hasher,
}

impl Default for SHA512_256Hasher {
    fn default() -> Self {
        Self {
            inner: SHA512Hasher::new_with_hash(&INITIAL_HASH_256),
        }
    }
}

impl Hasher for SHA512_256Hasher {
    fn block_size(&self) -> usize {
        self.inner.block_size()
    }

    fn output_size(&self) -> usize {
        32
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.update(data);
    }

    fn finish(&self) -> Vec<u8> {
        let mut out = self.inner.finish();
        out.truncate(self.output_size());
        out
    }

    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha512_test() {
        let h = |s: &str| {
            let mut hasher = SHA512Hasher::default();
            hasher.update(s.as_bytes());
            hasher.finish()
        };

        assert_eq!(&h("")[..],
				&hex!("cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e")[..]);
        assert_eq!(&h("The quick brown fox jumps over the lazy dog")[..],
				&hex!("07e547d9586f6a73f73fbac0435ed76951218fb7d0c8d788a309d785436bbb642e93a252a954f23912547d1e8a3b5ed6e1bfd7097821233fa0538f3db854fee6")[..])

        // TODO: Test partial updates
    }
}
