use crate::hasher::*;
use crate::md::*;
use generic_array::GenericArray;
use typenum::U64;

// TODO: Use https://en.wikipedia.org/wiki/Intel_SHA_extensions

const INITIAL_HASH: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

type HashState = [u32; 5];
type HashOutput = [u8; 20];

#[derive(Clone)]
pub struct SHA1Hasher {
    inner: MerkleDamgard<HashState, U64>,
}

impl SHA1Hasher {
    /// Internal utility for updating a SHA1 hash given a full chunk.
    fn update_chunk(chunk: &GenericArray<u8, U64>, hash: &mut HashState) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(*array_ref![chunk, 4 * i, 4]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = hash[0];
        let mut b = hash[1];
        let mut c = hash[2];
        let mut d = hash[3];
        let mut e = hash[4];

        for i in 0..80 {
            let (f, k) = if i < 20 {
                ((b & c) | ((!b) & d), 0x5A827999)
            } else if i < 40 {
                (b ^ c ^ d, 0x6ED9EBA1)
            } else if i < 60 {
                ((b & c) | (b & d) | (c & d), 0x8F1BBCDC)
            } else {
                (b ^ c ^ d, 0xCA62C1D6)
            };

            let tmp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = tmp;
        }

        // TODO: Vectorize
        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
    }
}

impl Default for SHA1Hasher {
    fn default() -> Self {
        let padding = LengthPadding {
            big_endian: true,
            int128: false,
        };
        Self {
            inner: MerkleDamgard::new(INITIAL_HASH, padding),
        }
    }
}

impl Hasher for SHA1Hasher {
    fn output_size(&self) -> usize {
        20
    }

    fn update(&mut self, data: &[u8]) {
        self.inner.update(data, Self::update_chunk);
    }

    fn finish(&self) -> Vec<u8> {
        let state = self.inner.finish(Self::update_chunk);

        // Generate final message by casting to big endian
        let mut hh = [0u8; 20];
        *array_mut_ref![hh, 0, 4] = state[0].to_be_bytes();
        *array_mut_ref![hh, 4, 4] = state[1].to_be_bytes();
        *array_mut_ref![hh, 8, 4] = state[2].to_be_bytes();
        *array_mut_ref![hh, 12, 4] = state[3].to_be_bytes();
        *array_mut_ref![hh, 16, 4] = state[4].to_be_bytes();
        hh.to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::hex;

    #[test]
    fn sha1_test() {
        let h = |s: &str| {
            let mut hasher = SHA1Hasher::default();
            hasher.update(s.as_bytes());
            hasher.finish()
        };

        assert_eq!(
            &h("")[..],
            &hex::decode("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap()[..]
        );
        assert_eq!(
            &h("The quick brown fox jumps over the lazy dog")[..],
            &hex::decode("2fd4e1c67a2d28fced849ee1bb76e7391b93eb12").unwrap()[..]
        )

        // TODO: Test partial updates
    }
}
