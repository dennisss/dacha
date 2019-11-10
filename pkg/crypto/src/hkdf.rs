use crate::hasher::*;
use crate::hmac::*;
use common::ceil_div;

/// https://tools.ietf.org/html/rfc5869
pub struct HKDF {
	hasher_factory: HasherFactory,
	hash_len: usize
}

impl HKDF {
	pub fn new(hasher_factory: HasherFactory) -> Self {
		// NOTE: This should be output size of HMAC (not of the hash necessarily).
		let hash_len = hasher_factory.create().output_size();
		Self { hasher_factory, hash_len }
	}

	pub fn hash_size(&self) -> usize {
		self.hash_len
	}

	pub fn extract(&self, salt: &[u8], ikm: &[u8]) -> Vec<u8> {
		let mut hmac = HMAC::new(self.hasher_factory.box_clone(), salt);
		hmac.update(ikm);
		hmac.finish()
	}

	/// Returns the OKM (output keying material).
	pub fn expand(&self, prk: &[u8], info: &[u8], l: usize) -> Vec<u8> {
		let n = ceil_div(l, self.hash_len);
		assert!(n <= 255);

		let mut t = vec![];
		t.reserve(n*self.hash_len);
		for i in 0..n {
			let mut hmac = HMAC::new(self.hasher_factory.box_clone(), prk);
			if i > 0 {
				let ii = (i - 1)*self.hash_len;
				let jj = ii + self.hash_len;
				hmac.update(&t[ii..jj]);
			}

			hmac.update(info);
			
			let mut idx = [0u8; 1];
			idx[0] = (i + 1) as u8;
			hmac.update(&idx);

			t.extend_from_slice(&hmac.finish());
		}

		t.truncate(l);
		t
	}
}

impl Clone for HKDF {
	fn clone(&self) -> Self {
		Self {
			hasher_factory: self.hasher_factory.box_clone(),
			hash_len: self.hash_len }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::sha256::*;

	#[test]
	fn hkdf_test() {

		let hkdf_sha256 = HKDF::new(SHA256Hasher::factory());

		let prk = hkdf_sha256.extract(
			&hex::decode("000102030405060708090a0b0c").unwrap(),
			&hex::decode("0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b0b").unwrap());

		let okm = hkdf_sha256.expand(&prk,
			&hex::decode("f0f1f2f3f4f5f6f7f8f9").unwrap(), 42);


		assert_eq!(&prk[..],
				&hex::decode("077709362c2e32df0ddc3f0dc47bba6390b6c73bb50f9c3122ec844ad7c2b3e5").unwrap()[..]);
		assert_eq!(&okm[..],
				&hex::decode("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865").unwrap()[..]);
	}

}