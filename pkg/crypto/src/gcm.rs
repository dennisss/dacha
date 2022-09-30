use alloc::boxed::Box;
use std::string::ToString;
use std::vec::Vec;

use common::errors::*;
use math::big::BigUint;
use math::integer::Integer;
use math::number::Zero;

use crate::aead::*;
use crate::aes::*;
use crate::cipher::*;
use crate::constant_eq;
use crate::utils::*;

type Block = [u8; 16];
const BLOCK_SIZE: usize = 16;

// TODO: THis desperately needs to be more efficient.

/// Operations of the field GF(2^m).
struct GaloisField2 {
    m: usize,
    poly: BigUint,
}

impl GaloisField2 {
    /// Creates a new field using the given irreducible polynomial where each
    /// bit corresponds to a power of 2.
    ///
    /// NOTE: The number 'm' is uniquely for the polynomial as it is
    /// irreducible.
    pub fn new(m: usize, poly: BigUint) -> Self {
        Self { m, poly }
    }

    /// Creates the GF(2^128) field used by GCM.
    ///
    /// Reduces by the polynomial 'x^128 + x^7 + x^2 + x + 1'
    pub fn gcm128() -> Self {
        let p = {
            let mut n = BigUint::zero();
            n.set_bit(128, 1);
            n.set_bit(7, 1);
            n.set_bit(2, 1);
            n.set_bit(1, 1);
            n.set_bit(0, 1);
            n
        };

        Self::new(128, p)
    }

    // NOTE: This is not defined in any specific field.
    // TODO: Copied from biguint for composite field usage.
    fn rem(lhs: BigUint, rhs: &BigUint) -> BigUint {
        if lhs < *rhs {
            return lhs;
        }

        let mut r = BigUint::zero();
        for i in (0..lhs.value_bits()).rev() {
            r.shl();
            r.set_bit(0, lhs.bit(i));
            if r >= *rhs {
                r ^= rhs; // GF(2^m) subtraction.
            }
        }

        r
    }

    // TODO: Not cryptographically secure.
    pub fn mul(&self, mut a: BigUint, b: &BigUint) -> BigUint {
        // Multiple one bit at a time.
        let mut out = BigUint::zero();
        for i in 0..b.value_bits() {
            if b.bit(i) == 1 {
                out ^= &a;
            }

            a.shl();
        }

        // Reduce mod polynomial.
        Self::rem(out, &self.poly)
    }

    pub fn reflected_mul(&self, a: BigUint, b: &BigUint) -> BigUint {
        let mut ar = BigUint::zero();
        let mut br = BigUint::zero();
        for i in 0..128 {
            ar.set_bit(i, a.bit(127 - i));
            br.set_bit(i, b.bit(127 - i));
        }

        let outr = self.mul(ar, &br);
        let mut out = BigUint::zero();
        for i in 0..128 {
            out.set_bit(i, outr.bit(127 - i));
        }

        out
    }
}

/// Applies a function over every 16byte block of some data. If the last block
/// is incomplete, it is padded to the right with zeros.
fn map_blocks<F: FnMut(&Block)>(data: &[u8], mut f: F) {
    let n = data.len() / BLOCK_SIZE;
    let r = data.len() % BLOCK_SIZE;

    for i in 0..n {
        f(array_ref![data, BLOCK_SIZE * i, BLOCK_SIZE]);
    }

    if r != 0 {
        let mut block = [0u8; BLOCK_SIZE];
        block[0..r].copy_from_slice(&data[(data.len() - r)..]);
        f(&block);
    }
}

/*
    For 128bit ciphers
    IV: recomended 96bit
*/

struct GaloisCounterMode<C: BlockCipher> {
    cipher: C,

    /// Current value of the counter concatenated to the end of the IV.
    counter: Block,

    enc_counter_0: Block,

    /// E(K, 0^128)
    /// TODO: Move ownership to the GHasher
    h: BigUint,
}

impl<C: BlockCipher> GaloisCounterMode<C> {
    pub fn new(iv: &[u8], cipher: C) -> Self {
        // Only defined for 128bit ciphers.
        assert_eq!(cipher.block_size(), 16);

        let h = {
            let data = [0u8; 16];
            let mut enc = [0u8; 16];
            cipher.encrypt_block(&data, &mut enc);
            BigUint::from_be_bytes(&enc)
        };

        let counter = if iv.len() == 12 {
            let mut data = [0u8; 16];
            data[0..12].copy_from_slice(iv);
            data[15] = 1;
            data
        } else {
            Self::ghash(&h, &[], iv)
        };

        let mut enc_counter_0 = [0u8; 16];
        cipher.encrypt_block(&counter, &mut enc_counter_0);

        Self {
            cipher,
            counter,
            enc_counter_0,
            h,
        }
    }

    fn incr(data: &mut Block) {
        let mut i = u32::from_be_bytes(*array_ref![data, 12, 4]);
        i = i.wrapping_add(1);
        *array_mut_ref![data, 12, 4] = i.to_be_bytes();
    }

    fn ghash(h: &BigUint, a: &[u8], c: &[u8]) -> Block {
        let mut x = BigUint::zero(); // X_0

        let mut hasher = GHasher::new(h.clone());

        map_blocks(a, |block| hasher.update(block));
        map_blocks(c, |block| hasher.update(block));

        hasher.finish(a.len(), c.len())
    }

    pub fn encrypt(&mut self, plain: &[u8], additional_data: &[u8], output: &mut Vec<u8>) {
        // TODO: Remove this restriction
        // And let's allocate all the bytes we need ahead of time.
        // assert_eq!(output.len(), 0);
        let initial_length = output.len();

        let mut hasher = GHasher::new(self.h.clone());
        map_blocks(additional_data, |block| hasher.update(block));

        map_blocks(plain, |p| {
            Self::incr(&mut self.counter);

            let output_start = output.len();
            output.resize(output.len() + BLOCK_SIZE, 0);
            let output_block = &mut output[output_start..];
            self.cipher.encrypt_block(&self.counter, output_block);

            xor_inplace(p, output_block);

            // If we are doing the last block, set all cipher bytes after the
            // end of the plaintext to zero.
            for i in plain.len()..(output_start + BLOCK_SIZE) {
                output_block[i - output_start] = 0;
            }

            hasher.update(array_ref![output_block, 0, BLOCK_SIZE]);
        });

        output.truncate(initial_length + plain.len());

        let mut tag = hasher.finish(additional_data.len(), output.len() - initial_length);
        xor_inplace(&self.enc_counter_0, &mut tag);

        output.extend_from_slice(&tag);
    }

    pub fn decrypt(
        &mut self,
        auth_cipher: &[u8],
        additional_data: &[u8],
        plain: &mut Vec<u8>,
    ) -> Result<()> {
        // Must have enough bytes for the tag.
        if auth_cipher.len() < BLOCK_SIZE {
            return Err(err_msg("Invalid ciphertext size"));
        }

        let (cipher, tag) = auth_cipher.split_at(auth_cipher.len() - BLOCK_SIZE);

        let mut hasher = GHasher::new(self.h.clone());
        map_blocks(additional_data, |block| hasher.update(block));

        map_blocks(cipher, |c| {
            hasher.update(c);

            Self::incr(&mut self.counter);

            plain.resize(plain.len() + BLOCK_SIZE, 0);
            let plain_len = plain.len();
            let plain_block = &mut plain[(plain_len - 16)..];
            self.cipher.encrypt_block(&self.counter, plain_block);

            xor_inplace(c, plain_block);
        });

        // Must truncate output if last block is incomplete.
        // TODO: Assumes output buffere was initially empty.
        plain.truncate(cipher.len());

        let mut expected_tag = hasher.finish(additional_data.len(), cipher.len());
        xor_inplace(&self.enc_counter_0, &mut expected_tag);

        if !constant_eq(tag, &expected_tag) {
            // println!("{:?}\n{:?}", tag, expected_tag);
            return Err(err_msg("Incorrect tag"));
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct AesGCM {
    key_size: usize,
}

impl AesGCM {
    pub fn aes128() -> Self {
        Self {
            key_size: (128 / 8),
        }
    }
    pub fn aes256() -> Self {
        Self {
            key_size: (256 / 8),
        }
    }
}

impl AuthEncAD for AesGCM {
    fn key_size(&self) -> usize {
        self.key_size
    }

    // See https://datatracker.ietf.org/doc/html/rfc5116#section-5.1
    //
    // NOTE: Technically any size of nonce is valid, but to get TLS to use the
    // recommended size, we fix it.
    fn nonce_range(&self) -> (usize, usize) {
        (12, 12)
    }

    fn expanded_size(&self, plaintext_size: usize) -> usize {
        plaintext_size + 16
    }

    fn encrypt(
        &self,
        key: &[u8],
        nonce: &[u8],
        plaintext: &[u8],
        additional_data: &[u8],
        out: &mut Vec<u8>,
    ) {
        assert_eq!(key.len(), self.key_size);
        let c = AESBlockCipher::create(key).unwrap();
        let mut gcm = GaloisCounterMode::new(nonce, c);
        gcm.encrypt(plaintext, additional_data, out);
    }

    fn decrypt(
        &self,
        key: &[u8],
        nonce: &[u8],
        ciphertext: &[u8],
        additional_data: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<()> {
        assert_eq!(key.len(), self.key_size);
        let c = AESBlockCipher::create(key).unwrap();
        let mut gcm = GaloisCounterMode::new(nonce, c);
        gcm.decrypt(ciphertext, additional_data, out)
    }

    fn box_clone(&self) -> Box<dyn AuthEncAD> {
        Box::new(self.clone())
    }
}

/// Computed the
pub struct GHasher {
    x: BigUint,

    /// E(K, 0^128)
    h: BigUint,

    field: GaloisField2,
}

impl GHasher {
    pub fn new(h: BigUint) -> Self {
        Self {
            x: BigUint::zero(),
            h,
            field: GaloisField2::gcm128(),
        }
    }

    pub fn reset(&mut self) {
        self.x = BigUint::zero();
    }

    fn update_with(&self, mut x: BigUint, block: &Block) -> BigUint {
        let b = BigUint::from_be_bytes(block);
        x ^= b;
        self.field.reflected_mul(x, &self.h)
    }

    /// Should be called first with all blocks of authenticated data and then
    /// with all blocks of ciphertext.
    pub fn update(&mut self, block: &Block) {
        let mut x = BigUint::zero();
        std::mem::swap(&mut x, &mut self.x);
        self.x = self.update_with(x, block);
    }

    /// Given the length of the authenticated data and the length of the
    /// ciphertext both in byte units, get the final authenticated tag.
    pub fn finish(&self, a_len: usize, c_len: usize) -> Block {
        let mut last_block = [0u8; 16];
        *array_mut_ref![last_block, 0, 8] = ((a_len * 8) as u64).to_be_bytes();
        *array_mut_ref![last_block, 8, 8] = ((c_len * 8) as u64).to_be_bytes();

        let mut x = self.x.clone();
        x = self.update_with(x, &last_block);

        let mut out = [0u8; 16];
        let hash = x.to_be_bytes();
        out[(16 - hash.len())..].copy_from_slice(&hash);
        out
    }
}

// A_1, ... A_m
// P_1, ... P_n
// C_1, ... C_n

// GHASH(H, A, C) = X_(m+n+1)

// GHASH(H, {}, IV)

#[cfg(test)]
mod tests {
    use super::*;
    use common::hex;

    // TODO: Must test for partial encryption/decryption of plaintext that
    // doesn't fill an exact number of blocks.

    #[test]
    fn gfmul128_test() {
        // Test vector from Intel's Whitepaper
        // a = 0x7b5b54657374566563746f725d53475d
        // b = 0x48692853686179295b477565726f6e5d
        // GFMUL128 (a, b) = 0x40229a09a5ed12e7e4e10da323506d2

        let a = BigUint::from_be_bytes(&hex::decode("7b5b54657374566563746f725d53475d").unwrap());
        let b = BigUint::from_be_bytes(&hex::decode("48692853686179295b477565726f6e5d").unwrap());
        let c = BigUint::from_be_bytes(&hex::decode("040229a09a5ed12e7e4e10da323506d2").unwrap());

        let field = GaloisField2::gcm128();
        assert_eq!(field.mul(a, &b).to_string(), c.to_string());
    }

    // TODO: See https://boringssl.googlesource.com/boringssl/+/2214/crypto/cipher/cipher_test.txt for a nice list of test vector.
    // of the form:
    // 'cipher:key:iv:plaintext:ciphertext:aad:tag'

    // AES-128-GCM:feffe9928665731c6d6a8f9467308308:cafebabefacedbaddecaf888:
    // d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255:
    // 42831ec2217774244b7221b784d0d49ce3aa212f2c02a4e035c17e2329aca12e21d514b25466931c7d8f6a5aac84aa051ba30b396a0aac973d58e091473f5985:
    // :4d5c2af327cd64a62cf35abd2ba6fab4

    #[test]
    fn gcm_test() {
        // Test Case 3 from the original GCM paper

        let k = hex::decode("feffe9928665731c6d6a8f9467308308").unwrap();
        let p = hex::decode(
			"d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255").unwrap();
        let iv = hex::decode("cafebabefacedbaddecaf888").unwrap();

        // NOTE: Final 4d5c2af327cd64a62cf35abd2ba6fab4 is the tag.
        let cipher = hex::decode("42831ec2217774244b7221b784d0d49ce3aa212f2c02a4e035c17e2329aca12e21d514b25466931c7d8f6a5aac84aa051ba30b396a0aac973d58e091473f59854d5c2af327cd64a62cf35abd2ba6fab4").unwrap();

        let mut out = vec![];
        let mut gcm = GaloisCounterMode::new(&iv, AESBlockCipher::create(&k).unwrap());
        gcm.encrypt(&p, &[], &mut out);

        assert_eq!(&out, &cipher);
    }

    #[test]
    fn gcm_unaligned_test() {
        // From NIST test vectors.
        let key = hex::decode("1694029fc6c85dad8709fd4568ebf99c").unwrap();
        let iv = hex::decode("d2c27040b28a9c31af6dad0a").unwrap();
        let cipher =
            hex::decode("e17df7ed1b0c36c6bab1c21dc108644413f80753a66d27cc37d9903abf").unwrap();
        let add_data = b"";
        let plain = hex::decode("51756d23ab2b2c4d4609e3133a").unwrap();

        let mut out = vec![];
        let aes_gcm = AesGCM::aes128();
        aes_gcm
            .decrypt(&key, &iv, &cipher, add_data, &mut out)
            .unwrap();

        assert_eq!(out, plain);

        out.clear();
        aes_gcm.encrypt(&key, &iv, &plain, add_data, &mut out);
        assert_eq!(out, cipher);
    }

    #[async_std::test]
    async fn aes_gcm_nist_test() -> Result<()> {
        let project_dir = common::project_dir();

        let paths = &[
            "testdata/nist/aes_gcm/gcmDecrypt128.rsp",
            // "testdata/nist/aes_gcm/gcmDecrypt192.rsp",
            "testdata/nist/aes_gcm/gcmDecrypt256.rsp",
            "testdata/nist/aes_gcm/gcmEncryptExtIV128.rsp",
            // "testdata/nist/aes_gcm/gcmEncryptExtIV192.rsp",
            "testdata/nist/aes_gcm/gcmEncryptExtIV256.rsp",
        ];

        for path in paths.iter().cloned() {
            // println!("FILE {}", path);

            let file = crate::nist::response::ResponseFile::open(project_dir.join(path)).await?;

            for response in file.iter() {
                let response = response?;

                // println!("Response {}", response.fields["COUNT"]);

                let fail = response.fields.contains_key("FAIL");

                let key = hex::decode(response.fields.get("KEY").unwrap())?;
                let iv = hex::decode(response.fields.get("IV").unwrap())?;
                let plaintext = {
                    if let Some(data) = response.fields.get("PT") {
                        hex::decode(data)?
                    } else {
                        vec![]
                    }
                };
                let additional_data = hex::decode(response.fields.get("AAD").unwrap())?;
                let ciphertext = hex::decode(response.fields.get("CT").unwrap())?;
                let tag = hex::decode(response.fields.get("TAG").unwrap())?;

                if tag.len() != 16 {
                    continue;
                }

                let aes = {
                    if key.len() == 16 {
                        AesGCM::aes128()
                    } else {
                        AesGCM::aes256()
                    }
                };

                let full_ciphertext = {
                    let mut v = ciphertext.clone();
                    v.extend_from_slice(&tag);
                    v
                };

                // NOTE: Even though the files are only for just one of (encryption,
                // decryption), we try both directions always for thoroughness.

                // Decrypt
                {
                    let mut out = vec![];
                    let result =
                        aes.decrypt(&key, &iv, &full_ciphertext, &additional_data, &mut out);
                    assert_eq!(result.is_err(), fail);
                    if !fail {
                        assert_eq!(out, plaintext);
                    }
                }

                // Encrypt
                if !fail {
                    let mut out = vec![];
                    aes.encrypt(&key, &iv, &plaintext, &additional_data, &mut out);
                    assert_eq!(out, full_ciphertext);
                }
            }
        }

        Ok(())
    }
}
