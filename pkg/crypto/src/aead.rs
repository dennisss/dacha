
// TODO: For ciphers like AES-GCM, the size of the plaintext can't be decrypted.

trait AuthEncAD {

	fn key_size(&self) -> usize;

	fn nonce_size(&self) -> (usize, usize);

	fn encrypt(&self, key: &[u8], nonce: &[u8], plaintext: &[u8],
			   additional_data: &[u8], out: &mut Vec<u8>);

	fn decrypt(&self, key: &[u8], nonce: &[u8], ciphertext: &[u8],
			   additional_data: &[u8], out: &mut Vec<u8>);
}