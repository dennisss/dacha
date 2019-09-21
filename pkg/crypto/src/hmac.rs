use crate::hasher::*;
use crate::utils::*;

/*
Standard block sizes:
MD5: 64 bytes
SHA1: 64 bytes
SHA256: 64 bytes
*/

// TODO: Make dynamic
const BLOCK_SIZE: usize = 64;

/// https://tools.ietf.org/html/rfc2104
pub struct HMAC {
	// TODO: The size of this is bounded by the block size assuming block size
	// <= output size of the hash.
	derived_key: [u8; BLOCK_SIZE],
	
	hash: HasherFactory,

	/// Underlying hashing function used as the inner hasher.
	inner_hasher: Box<dyn Hasher>
}

impl HMAC {
	pub fn new(hash: HasherFactory, key: &[u8]) -> Self {
		let mut derived_key = [0u8; BLOCK_SIZE];
		if key.len() <= BLOCK_SIZE {
			derived_key[0..key.len()].copy_from_slice(key);
		} else {
			let mut h = hash.create();
			h.update(key);
			let key_hash = h.finish();
			derived_key[0..key_hash.len()]
				.copy_from_slice(&key_hash);
		};

		let mut inner_hasher = hash.create();

		// Initialize inner hash with 'derived_key xor ipad'.
		let mut inner_start = [0u8; BLOCK_SIZE];
		let ipad = [0x36u8; BLOCK_SIZE];
		xor(&ipad, &derived_key, &mut inner_start);
		inner_hasher.update(&inner_start);

		Self { hash, derived_key, inner_hasher }
	}

	pub fn output_size(&self) -> usize {
		// NOTE: This assumes that the inner and outer hashes are the same.
		self.inner_hasher.output_size()
	}

	pub fn update(&mut self, data: &[u8]) {
		self.inner_hasher.update(data);
	}

	pub fn finish(&self) -> Vec<u8> {
		let mut outer_hasher = self.hash.create();

		// Initialize outer hasher with 'derived_key xor opad'
		let mut outer_start = [0u8; BLOCK_SIZE];
		let opad = [0x5cu8; BLOCK_SIZE];
		xor(&opad, &self.derived_key, &mut outer_start);
		outer_hasher.update(&outer_start);
		
		outer_hasher.update(self.inner_hasher.finish().as_ref());
		outer_hasher.finish()
	}
}


#[cfg(test)]
mod tests {
	use super::*;
	use crate::md5::MD5Hasher;
	use crate::sha1::SHA1Hasher;
	use crate::sha256::SHA256Hasher;

	#[test]
	fn hmac_test() {
		let mut hmac1 = HMAC::new(MD5Hasher::factory(), b"key");
		hmac1.update(b"The quick brown fox jumps over the lazy dog");
		assert_eq!(&hmac1.finish()[..],
				   &hex::decode("80070713463e7749b90c2dc24911e275").unwrap()[..]);

		let mut hmac2 = HMAC::new(SHA1Hasher::factory(), b"key");
		hmac2.update(b"The quick brown fox jumps over the lazy dog");
		assert_eq!(&hmac2.finish()[..],
				   &hex::decode("de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9").unwrap()[..]);

		let mut hmac3 = HMAC::new(SHA256Hasher::factory(), b"key");
		hmac3.update(b"The quick brown fox jumps over the lazy dog");
		assert_eq!(&hmac3.finish()[..],
				   &hex::decode("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8").unwrap()[..]);

		// TODO: Test partial updates
	}

}