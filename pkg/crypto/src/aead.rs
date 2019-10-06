use common::errors::*;

// TODO: For ciphers like AES-GCM, the size of the plaintext can't be decrypted.

/// Authenticated Encryption with Additional Data.
pub trait AuthEncAD: Send {

	// TODO: This should be refactored as typically a lot of stuff can be
	// precomputed if using a fixed key and varied nonce.

	fn key_size(&self) -> usize;

	fn nonce_range(&self) -> (usize, usize);

	fn encrypt(&self, key: &[u8], nonce: &[u8], plaintext: &[u8],
			   additional_data: &[u8], out: &mut Vec<u8>);

	fn decrypt(&self, key: &[u8], nonce: &[u8], ciphertext: &[u8],
			   additional_data: &[u8], out: &mut Vec<u8>) -> Result<()>;
}