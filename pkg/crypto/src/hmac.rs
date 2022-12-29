use alloc::boxed::Box;
use std::vec::Vec;

use crate::hasher::*;
use crate::utils::*;

/// https://tools.ietf.org/html/rfc2104
pub struct HMAC {
    // TODO: The size of this is bounded by the block size assuming block size
    // <= output size of the hash.
    derived_key: Vec<u8>,

    hash: HasherFactory,

    /// Underlying hashing function used as the inner hasher.
    inner_hasher: Box<dyn Hasher>,
}

impl HMAC {
    pub fn new(hash: HasherFactory, key: &[u8]) -> Self {
        let block_size = hash.create().block_size();

        let mut derived_key = vec![0u8; block_size];
        if key.len() <= block_size {
            derived_key[0..key.len()].copy_from_slice(key);
        } else {
            let key_hash = hash.create().finish_with(key);
            derived_key[0..key_hash.len()].copy_from_slice(&key_hash);
        };

        let mut inner_hasher = hash.create();

        // Initialize inner hash with 'derived_key xor ipad'.
        let mut inner_start = vec![0u8; block_size];
        let ipad = vec![0x36u8; block_size];
        xor(&ipad, &derived_key, &mut inner_start);
        inner_hasher.update(&inner_start);

        Self {
            hash,
            derived_key,
            inner_hasher,
        }
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
        let block_size = outer_hasher.block_size();

        // Initialize outer hasher with 'derived_key xor opad'
        let mut outer_start = vec![0u8; block_size];
        let opad = vec![0x5cu8; block_size];
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
    use common::errors::*;

    #[test]
    fn hmac_test() {
        let mut hmac1 = HMAC::new(MD5Hasher::factory(), b"key");
        hmac1.update(b"The quick brown fox jumps over the lazy dog");
        assert_eq!(
            &hmac1.finish()[..],
            &hex!("80070713463e7749b90c2dc24911e275")[..]
        );

        let mut hmac2 = HMAC::new(SHA1Hasher::factory(), b"key");
        hmac2.update(b"The quick brown fox jumps over the lazy dog");
        assert_eq!(
            &hmac2.finish()[..],
            &hex!("de7c9b85b8b78aa6bc8a7a36f70a90701c9db4d9")[..]
        );

        let mut hmac3 = HMAC::new(SHA256Hasher::factory(), b"key");
        hmac3.update(b"The quick brown fox jumps over the lazy dog");
        assert_eq!(
            &hmac3.finish()[..],
            &hex!("f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8")[..]
        );

        // TODO: Test partial updates
    }

    #[testcase]
    async fn hmac_nist_test() -> Result<()> {
        let file =
            crate::nist::response::ResponseFile::open(project_path!("testdata/nist/hmac/HMAC.rsp"))
                .await?;

        for response in file.iter() {
            let response = response?;

            let hash_length = response.attributes["L"].parse::<usize>()?;

            let key_length = response.fields["KLEN"].parse::<usize>()?;
            let mac_length = response.fields["TLEN"].parse::<usize>()?;

            let key = radix::hex_decode(response.fields.get("KEY").unwrap())?;
            let message = radix::hex_decode(response.fields.get("MSG").unwrap())?;
            let mac = radix::hex_decode(response.fields.get("MAC").unwrap())?;

            let hasher_factory = match hash_length {
                20 => crate::sha1::SHA1Hasher::factory(),
                28 => crate::sha224::SHA224Hasher::factory(),
                32 => crate::sha256::SHA256Hasher::factory(),
                48 => crate::sha384::SHA384Hasher::factory(),
                64 => crate::sha512::SHA512Hasher::factory(),
                _ => panic!("Unsupported hash length in NIST test vectors"),
            };

            let mut hmac = HMAC::new(hasher_factory, &key[0..key_length]);
            hmac.update(&message);
            let output = hmac.finish();

            assert_eq!(&output[0..mac_length], mac);
        }

        Ok(())
    }
}
