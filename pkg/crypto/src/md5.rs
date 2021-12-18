use alloc::boxed::Box;
use alloc::vec::Vec;

use generic_array::GenericArray;
use typenum::U64;

use crate::hasher::*;
use crate::md::*;

/// Per-round shift amounts.
const SHIFTS: [u8; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9,
    14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15,
    21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

/// Constants from sines of integer indices.
/// Computed as:
/// K[i] = floor(2^32 × abs(sin(i + 1)))
const K_SINES: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

const A0: u32 = 0x67452301;
const B0: u32 = 0xefcdab89;
const C0: u32 = 0x98badcfe;
const D0: u32 = 0x10325476;

type HashState = [u32; 4];

#[derive(Clone)]
pub struct MD5Hasher {
    inner: MerkleDamgard<HashState, U64>,
}

impl MD5Hasher {
    fn update_chunk(data: &GenericArray<u8, U64>, hash: &mut HashState) {
        let mut M = [0u32; 16];
        for i in 0..16 {
            M[i] = u32::from_le_bytes(*array_ref![data, 4 * i, 4]);
        }

        let mut A = hash[0];
        let mut B = hash[1];
        let mut C = hash[2];
        let mut D = hash[3];

        for i in 0..64 {
            let (mut F, g) = match i {
                0..16 => ((B & C) | ((!B) & D), i),
                16..32 => ((D & B) | ((!D) & C), (5 * i + 1) % 16),
                32..48 => (B ^ C ^ D, (3 * i + 5) % 16),
                /* 48..64 */ _ => (C ^ (B | (!D)), (7 * i) % 16),
            };

            F = F
                .wrapping_add(A)
                .wrapping_add(K_SINES[i])
                .wrapping_add(M[g]);
            A = D;
            D = C;
            C = B;
            B = B.wrapping_add(F.rotate_left(SHIFTS[i] as u32));
        }

        hash[0] = hash[0].wrapping_add(A);
        hash[1] = hash[1].wrapping_add(B);
        hash[2] = hash[2].wrapping_add(C);
        hash[3] = hash[3].wrapping_add(D);
    }
}

impl Default for MD5Hasher {
    fn default() -> Self {
        let padding = LengthPadding {
            big_endian: false,
            int128: false,
        };
        Self {
            inner: MerkleDamgard::new([A0, B0, C0, D0], padding),
        }
    }
}

impl Hasher for MD5Hasher {
    fn block_size(&self) -> usize {
        64
    }

    fn output_size(&self) -> usize {
        16
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.update(data, Self::update_chunk);
    }

    #[cfg(feature = "std")]
    fn finish(&self) -> Vec<u8> {
        let state = self.inner.finish(Self::update_chunk);

        let mut hh = [0u8; 16];
        *array_mut_ref![hh, 0, 4] = state[0].to_le_bytes();
        *array_mut_ref![hh, 4, 4] = state[1].to_le_bytes();
        *array_mut_ref![hh, 8, 4] = state[2].to_le_bytes();
        *array_mut_ref![hh, 12, 4] = state[3].to_le_bytes();
        hh.to_vec()
    }

    #[cfg(feature = "std")]
    fn box_clone(&self) -> Box<dyn Hasher> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::hex;

    #[test]
    fn md5_test() {
        let h = |s: &str| {
            let mut hasher = MD5Hasher::default();
            hasher.update(s.as_bytes());
            hasher.finish()
        };

        assert_eq!(
            &h("")[..],
            &hex::decode("d41d8cd98f00b204e9800998ecf8427e").unwrap()[..]
        );
        assert_eq!(
            &h("The quick brown fox jumps over the lazy dog")[..],
            &hex::decode("9e107d9d372bb6826bd81d3542a419d6").unwrap()[..]
        );
        assert_eq!(
            &h("The quick brown fox jumps over the lazy dog.")[..],
            &hex::decode("e4d909c290d0fb1ca068ffaddf22cbd0").unwrap()[..]
        );
        // TODO: Test partial updates
    }
}
