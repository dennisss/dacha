use crate::hasher::*;
use crate::md::*;

// TODO: Use https://en.wikipedia.org/wiki/Intel_SHA_extensions

const INITIAL_HASH: [u32; 8] = [
	0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
	0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19
];

const ROUND_CONSTANTS: [u32; 64] = [
	0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
	0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
	0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
	0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
	0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
	0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
	0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
	0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2
];

type HashState = [u32; 8];

#[derive(Clone)]
pub struct SHA256Hasher {
	inner: MerkleDamgard<HashState>
}


impl SHA256Hasher {
	/// Internal utility for updating a SHA1 hash given a full chunk.
	fn update_chunk(chunk: &ChunkData, hash: &mut HashState) {
		let mut w = [0u32; 64];
		for i in 0..16 {
			w[i] = u32::from_be_bytes(*array_ref![chunk, 4*i, 4]);
		}
		for i in 16..64 {
			let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18)
				^ (w[i-15] >> 3);
			let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19)
				^ (w[i-2] >> 10);
			w[i] = w[i-16].wrapping_add(s0)
				.wrapping_add(w[i-7]).wrapping_add(s1);
		}

		let mut a = hash[0];
		let mut b = hash[1];
		let mut c = hash[2];
		let mut d = hash[3];
		let mut e = hash[4];
		let mut f = hash[5];
		let mut g = hash[6];
		let mut h = hash[7];

		for i in 0..64 {
			// TODO: Use SHA256RNDS2
			let S1 = e.rotate_right(6) ^ e.rotate_right(11)
				^ e.rotate_right(25);
			let ch = (e & f) ^ ((!e) & g);
			let temp1 = h.wrapping_add(S1).wrapping_add(ch)
				.wrapping_add(ROUND_CONSTANTS[i])
				.wrapping_add(w[i]);
			let S0 = a.rotate_right(2) ^ a.rotate_right(13)
				^ a.rotate_right(22);
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

impl Default for SHA256Hasher {
	fn default() -> Self {
		Self { inner: MerkleDamgard::new(INITIAL_HASH, true) }
	}
}

impl Hasher for SHA256Hasher {
	fn output_size(&self) -> usize { 32 }

	fn update(&mut self, data: &[u8]) {
		self.inner.update(data, Self::update_chunk);
	}

	fn finish(&self) -> Vec<u8> {
		let state = self.inner.finish(Self::update_chunk);

		// Generate final message by casting to big endian
		let mut hh = [0u8; 32];
		for i in 0..(32 / 4) {
			*array_mut_ref![hh, 4*i, 4] = state[i].to_be_bytes();
		}

		hh.to_vec()
	}
} 


#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn sha256_test() {
		let h = |s: &str| {
			let mut hasher = SHA256Hasher::default();
			hasher.update(s.as_bytes());
			hasher.finish()
		};

		assert_eq!(&h("")[..],
				&hex::decode("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855").unwrap()[..]);
		assert_eq!(&h("The quick brown fox jumps over the lazy dog")[..],
				&hex::decode("d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592").unwrap()[..])

		// TODO: Test partial updates
	}

}