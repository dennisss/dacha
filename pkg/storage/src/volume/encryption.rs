use alloc::vec::Vec;

use common::errors::*;
use common::InRange;
use crypto::aead::AuthEncAD;
use crypto::checksum::crc::CRC32CHasher;
use crypto::gcm::AesGCM;
use crypto::hasher::{GetHasherFactory, Hasher, HasherFactory};
use crypto::hkdf::HKDF;
use crypto::sha512::SHA512Hasher;

use crate::proto::volume::*;

/// Encryption algorithm and key used for encoding data stored on a volume.
///
/// TOOD: Choose a better name.
pub struct VolumeCipher {
    root_key: Vec<u8>,
    key_generator: HKDF,
    cipher: AesGCM,
    usage: VolumeKeyUsage,
}

impl VolumeCipher {
    pub fn new(root_key: &[u8], usage: &VolumeKeyUsage) -> Result<Self> {
        let key_generator = match usage.key_derivation() {
            VolumeKeyUsage_KeyDerivationFunction::UNKNOWN => {
                return Err(err_msg("Unknown key derivation type"));
            }
            VolumeKeyUsage_KeyDerivationFunction::HKDF_SHA512 => HKDF::new(SHA512Hasher::factory()),
        };

        let cipher = match usage.cipher() {
            VolumeKeyUsage_EncryptionCipher::UNKNOWN => {
                return Err(err_msg("Unknown encryption type"));
            }
            VolumeKeyUsage_EncryptionCipher::AES_GCM_128 => AesGCM::aes128(),
        };

        if root_key.len() < cipher.key_size() {
            return Err(err_msg("Weak root key provided."));
        }

        if !(usage.nonce_size() as usize).in_range(cipher.nonce_range().0, cipher.nonce_range().1) {
            return Err(err_msg("Unsupported nonce size"));
        }

        Ok(Self {
            root_key: root_key.to_vec(),
            key_generator,
            cipher,
            usage: usage.clone(),
        })
    }

    pub fn decrypt(&self, ciphertext: &[u8], salt: &[u8]) -> Result<Vec<u8>> {
        // TODO: Check salt length.

        let mut key = self.key_generator.extract(salt, &self.root_key);
        if key.len() < self.cipher.key_size() {
            return Err(err_msg("Derived too short of an encryption key"));
        }
        key.truncate(self.cipher.key_size());

        if ciphertext.len() < self.usage.nonce_size() as usize {
            return Err(err_msg("Data too short"));
        }

        let (nonce, data) = ciphertext.split_at(self.usage.nonce_size() as usize);

        let mut out = vec![];
        self.cipher.decrypt(&key, nonce, data, &[], &mut out)?;

        Ok(out)
    }
}
