use crate::hasher::*;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use std::ops::Deref;

const INITIAL_HASH: [u32; 5] = [
	0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0
];

const CHUNK_SIZE: usize = 64;

#[derive(Clone)]
struct SHA1Hasher {
	hash: [u32; 5],
	/// Total number of *bytes* seen so far.
	length: usize,
	/// If the 
	pending_chunk: [u8; CHUNK_SIZE]
}

impl SHA1Hasher {
	pub fn new() -> Self {
		SHA1Hasher {
			hash: INITIAL_HASH,
			length: 0,
			pending_chunk: [0u8; CHUNK_SIZE]
		}
	}

	/// Internal utility for updating a SHA1 hash given a full chunk.
	fn update_chunk(hash: &mut [u32; 5], chunk: &[u8; CHUNK_SIZE]) {
		let mut w = [0u32; 80];
		for i in 0..16 {
			w[i] = u32::from_be_bytes(*array_ref![chunk, 4*i, 4]);
		}
		for i in 16..80 {
			w[i] = (w[i-3] ^ w[i-8] ^ w[i-14] ^ w[i-16]).rotate_left(1);
		}

		let mut a = hash[0];
		let mut b = hash[1];
		let mut c = hash[2];
		let mut d = hash[3];
		let mut e = hash[4];

		for i in 0..80 {
			let (f, k) =
				if i < 20 {
					((b & c) | ((!b) & d),
					0x5A827999)
				} else if i < 40 {
					(b ^ c ^ d,
					0x6ED9EBA1)
				} else if i < 60 {
					((b & c) | (b & d) | (c & d),
					0x8F1BBCDC)
				} else {
					(b ^ c ^ d,
					0xCA62C1D6)
				};

			let tmp = a.rotate_left(5)
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

impl Hasher for SHA1Hasher {
	type Output = [u8; 20];

	fn update(&mut self, mut data: &[u8]) {
		// If the previous chunk isn't complete, try completing it.
		let rem = self.length % CHUNK_SIZE;
		if rem != 0 {
			let n = std::cmp::min(CHUNK_SIZE - rem, data.len());
			self.pending_chunk[rem..(rem + n)]
				.copy_from_slice(&data[0..n]);
			self.length += n;
			data = &data[n..];

			// Stop early if we did not fill the previous chunk.
			if self.length % CHUNK_SIZE != 0 {
				return;
			}

			Self::update_chunk(&mut self.hash, &self.pending_chunk);
		}

		// Process all full chunks in the new data.
		for i in 0..(data.len() / CHUNK_SIZE) {
			Self::update_chunk(&mut self.hash,
							   array_ref![data, CHUNK_SIZE*i, CHUNK_SIZE]);
		}

		// Copy any remaining data into the pending chunk.
		let r = data.len() % CHUNK_SIZE;
		self.pending_chunk[0..r].copy_from_slice(
			&data[(data.len() - r)..]);

		// Update the length
		self.length += data.len();
	}

	fn finish(&self) -> Self::Output {
		// This will be the message length appended to the end of the message.
		let message_length = 8*self.length as u64;

		// At least need enough space to append the '1' bit and the 64bit message length. Then we will pad this up to the next chunk boundary.
		// NOTE: This is only valid as we are only operating on byte boundaries.
		let mut padded_len = self.length + (1 + 8);
		padded_len += common::block_size_remainder(64, padded_len as u64) as usize; 
		// Number of extra bytes that need to be added to the message to fit the 1 bit, message length and padding.
		let num_extra = padded_len - self.length;

		println!("ADD EXTRA {}", num_extra);

		// Buffer allocated for the maximum number of extra bytes that we may need
		// we will only use num_extra at runtime.
		let mut buf = [0u8; CHUNK_SIZE + 9];

		buf[0] = 0x80;
		*array_mut_ref![buf, num_extra - 8, 8] = message_length.to_be_bytes();

		let mut h = self.clone();
		h.update(&buf[0..num_extra]);
		assert_eq!(h.length % CHUNK_SIZE, 0);

		// Generate final message by casting to big endian
		let mut hh = [0u8; 20];
		*array_mut_ref![hh, 0, 4] = h.hash[0].to_be_bytes();
		*array_mut_ref![hh, 4, 4] = h.hash[1].to_be_bytes();
		*array_mut_ref![hh, 8, 4] = h.hash[2].to_be_bytes();
		*array_mut_ref![hh, 12, 4] = h.hash[3].to_be_bytes();
		*array_mut_ref![hh, 16, 4] = h.hash[4].to_be_bytes();
		hh
	}
} 


#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn sha1_test() {
		let h = |s: &str| {
			let mut hasher = SHA1Hasher::new();
			hasher.update(s.as_bytes());
			hasher.finish()
		};

		assert_eq!(&h("")[..],
				&hex::decode("da39a3ee5e6b4b0d3255bfef95601890afd80709").unwrap()[..]);
		assert_eq!(&h("The quick brown fox jumps over the lazy dog")[..],
				&hex::decode("2fd4e1c67a2d28fced849ee1bb76e7391b93eb12").unwrap()[..])

		// TODO: Test partial updates
	}

}