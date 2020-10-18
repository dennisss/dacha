use crate::cipher::*;
use crate::utils::xor;
use common::errors::*;
use core::arch::x86_64::*;

// TODO: See also https://botan.randombit.net/doxygen/aes__ni_8cpp_source.html

const AES_BLOCK_SIZE: usize = 16;

type RoundKey = [u8; AES_BLOCK_SIZE];

const AES128_NUM_ROUNDS: usize = 10;

/// Generated by running through RoundConstantIter.
const AES128_ROUND_CONSTANTS: [i32; 11] = [1, 2, 4, 8, 16, 32, 64, 128, 27, 54, 108];

pub fn to_m128i(v: &[u8]) -> __m128i {
    assert_eq!(v.len(), 16);
    unsafe { _mm_loadu_si128(std::mem::transmute(v.as_ptr())) }
}

pub fn from_m128i(v: __m128i, out: &mut [u8]) {
    assert_eq!(out.len(), 16);
    unsafe {
        _mm_storeu_si128(std::mem::transmute(out.as_mut_ptr()), v);
    }
}

// https://en.wikipedia.org/wiki/Rijndael_key_schedule#Round_constants
struct RoundConstantIter {
    last: Option<u8>,
}

impl RoundConstantIter {
    fn new() -> Self {
        Self { last: None }
    }
}

impl std::iter::Iterator for RoundConstantIter {
    type Item = u8;
    fn next(&mut self) -> Option<Self::Item> {
        let v = if let Some(v) = self.last {
            let vv = v.wrapping_mul(2);
            if v < 0x80 {
                vv
            } else {
                vv ^ 0x1b
            }
        } else {
            1
        };
        self.last = Some(v);
        Some(v)
    }
}

// Based on https://github.com/Tarsnap/scrypt/blob/master/libcperciva/crypto/crypto_aes_aesni.c#L21
macro_rules! aes128_key_expand {
    ($rks:ident, $i:expr) => {{
        let mut _s = to_m128i(&$rks[$i - 1]);
        let mut _t = to_m128i(&$rks[$i - 1]);
        _s = _mm_xor_si128(_s, _mm_slli_si128(_s, 4));
        _s = _mm_xor_si128(_s, _mm_slli_si128(_s, 8));
        _t = _mm_aeskeygenassist_si128(_t, AES128_ROUND_CONSTANTS[$i - 1]);
        _t = _mm_shuffle_epi32(_t, 0xff);
        let key = _mm_xor_si128(_s, _t);

        let mut out: RoundKey = [0u8; AES_BLOCK_SIZE];
        from_m128i(key, &mut out);
        out
    }};
}

// Based on https://github.com/Tarsnap/scrypt/blob/master/libcperciva/crypto/crypto_aes_aesni.c#L75
macro_rules! aes256_key_expand {
    ($rks:ident, $i:expr, $shuffle:expr, $rcon:expr) => {{
        let mut _s = to_m128i(&$rks[$i - 2]);
        let mut _t = to_m128i(&$rks[$i - 1]);
        _s = _mm_xor_si128(_s, _mm_slli_si128(_s, 4));
        _s = _mm_xor_si128(_s, _mm_slli_si128(_s, 8));
        _t = _mm_aeskeygenassist_si128(_t, $rcon);
        _t = _mm_shuffle_epi32(_t, $shuffle);
        let key = _mm_xor_si128(_s, _t);

        let mut out: RoundKey = [0u8; AES_BLOCK_SIZE];
        from_m128i(key, &mut out);
        out
    }};
}

/// Encryptor/decrypter that uses one of AES-128/192/256 on
/// single blocks at a time. Typically you should use a stream
/// cipher that wraps this rather than using this directly.
pub struct AESBlockCipher {
    /// Round keys used for encrpytion.
    round_keys_enc: Vec<RoundKey>,
    /// Round keys used for decryption.
    round_keys_dec: Vec<RoundKey>,
}

impl AESBlockCipher {
    /// Creates a new cipher instance given a key of supported length.
    /// This will precompute all of the round keys for the key.
    pub fn create(key: &[u8]) -> Result<AESBlockCipher> {
        let round_keys_enc = if key.len() == 16 {
            Self::aes128_round_keys(key)
        } else if key.len() == 32 {
            Self::aes256_round_keys(key)
        } else {
            return Err(err_msg("Unsupported key length"));
        };

        let round_keys_dec = {
            let mut rks = vec![];
            rks.push(round_keys_enc[round_keys_enc.len() - 1]);
            let mut buf = [0u8; AES_BLOCK_SIZE];
            for i in 1..(round_keys_enc.len() - 1) {
                let k = unsafe {
                    _mm_aesimc_si128(to_m128i(&round_keys_enc[round_keys_enc.len() - i - 1]))
                };
                from_m128i(k, &mut buf);

                rks.push(buf.clone());
            }
            rks.push(round_keys_enc[0]);
            rks
        };

        Ok(Self {
            round_keys_enc,
            round_keys_dec,
        })
    }

    fn aes128_round_keys(key: &[u8]) -> Vec<RoundKey> {
        let mut out = vec![];
        out.reserve(AES128_NUM_ROUNDS + 1);

        unsafe {
            out.push(*array_ref![key, 0, 16]);
            out.push(aes128_key_expand!(out, 1));
            out.push(aes128_key_expand!(out, 2));
            out.push(aes128_key_expand!(out, 3));
            out.push(aes128_key_expand!(out, 4));
            out.push(aes128_key_expand!(out, 5));
            out.push(aes128_key_expand!(out, 6));
            out.push(aes128_key_expand!(out, 7));
            out.push(aes128_key_expand!(out, 8));
            out.push(aes128_key_expand!(out, 9));
            out.push(aes128_key_expand!(out, 10));
        }

        out
    }

    fn aes256_round_keys(key: &[u8]) -> Vec<RoundKey> {
        let mut out = vec![];
        // TODO: Reserve
        unsafe {
            out.push(*array_ref![key, 0, 16]);
            out.push(*array_ref![key, 16, 16]);
            out.push(aes256_key_expand!(out, 2, 0xff, 0x01));
            out.push(aes256_key_expand!(out, 3, 0xaa, 0x00));
            out.push(aes256_key_expand!(out, 4, 0xff, 0x02));
            out.push(aes256_key_expand!(out, 5, 0xaa, 0x00));
            out.push(aes256_key_expand!(out, 6, 0xff, 0x04));
            out.push(aes256_key_expand!(out, 7, 0xaa, 0x00));
            out.push(aes256_key_expand!(out, 8, 0xff, 0x08));
            out.push(aes256_key_expand!(out, 9, 0xaa, 0x00));
            out.push(aes256_key_expand!(out, 10, 0xff, 0x10));
            out.push(aes256_key_expand!(out, 11, 0xaa, 0x00));
            out.push(aes256_key_expand!(out, 12, 0xff, 0x20));
            out.push(aes256_key_expand!(out, 13, 0xaa, 0x00));
            out.push(aes256_key_expand!(out, 14, 0xff, 0x40));
        }

        out
    }
}

impl BlockCipher for AESBlockCipher {
    fn block_size(&self) -> usize {
        AES_BLOCK_SIZE
    }

    fn encrypt_block(&self, block: &[u8], out: &mut [u8]) {
        assert_eq!(block.len(), self.block_size());
        assert_eq!(block.len(), out.len());

        let rks = &self.round_keys_enc;

        let mut state = to_m128i(block);
        state = unsafe { _mm_xor_si128(state, to_m128i(&rks[0])) };

        for i in 1..(rks.len() - 1) {
            state = unsafe { _mm_aesenc_si128(state, to_m128i(&rks[i])) };
        }

        state = unsafe { _mm_aesenclast_si128(state, to_m128i(&rks[rks.len() - 1])) };

        from_m128i(state, out);
    }

    fn decrypt_block(&self, block: &[u8], out: &mut [u8]) {
        assert_eq!(block.len(), self.block_size());
        assert_eq!(block.len(), out.len());

        let rks = &self.round_keys_dec;

        let mut state = to_m128i(block);

        state = unsafe { _mm_xor_si128(state, to_m128i(&rks[0])) };

        for i in 1..(rks.len() - 1) {
            state = unsafe { _mm_aesdec_si128(state, to_m128i(&rks[i])) };
        }

        state = unsafe { _mm_aesdeclast_si128(state, to_m128i(&rks[rks.len() - 1])) };

        from_m128i(state, out);
    }
}

struct CBCModeCipher<C: BlockCipher> {
    cipher: C,
    iv: Vec<u8>,
}

macro_rules! next_block {
    ($data:ident, $size:expr) => {{
        let (block, rest) = $data.split_at($size);
        $data = rest;
        block
    }};
}

macro_rules! next_block_mut {
    ($data:ident, $size:expr) => {{
        let (block, rest) = $data.split_at_mut($size);
        $data = rest;
        block
    }};
}

impl<C: BlockCipher> CBCModeCipher<C> {
    pub fn encrypt(&mut self, mut input: &[u8], mut output: &mut [u8]) {
        assert_eq!(input.len() % self.cipher.block_size(), 0);
        assert_eq!(input.len(), output.len());

        let block_size = self.cipher.block_size();
        let nblocks = input.len() / block_size;

        // Intermediate buffer for storing the result of the xor
        let mut buf = vec![0; block_size];

        let mut iv: &[u8] = &self.iv;

        for i in 0..nblocks {
            let input_block = next_block!(input, block_size);
            let output_block = next_block_mut!(output, block_size);

            xor(iv, input_block, &mut buf);
            self.cipher.encrypt_block(&buf, output_block);
            iv = output_block;
        }
    }

    pub fn decrypt(&mut self, mut input: &[u8], mut output: &mut [u8]) {
        assert_eq!(input.len() % self.cipher.block_size(), 0);
        assert_eq!(input.len(), output.len());

        let block_size = self.cipher.block_size();
        let nblocks = input.len() / block_size;

        let mut buf = vec![0; block_size];

        let mut iv: &[u8] = &self.iv;

        for i in 0..nblocks {
            let input_block = next_block!(input, block_size);
            let output_block = next_block_mut!(output, block_size);

            self.cipher.decrypt_block(input_block, &mut buf);
            xor(&buf, iv, output_block);
            iv = input_block;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aes128_test() {
        let key = hex::decode("10a58869d74be5a374cf867cfb473859").unwrap();
        let plain = hex::decode("00000000000000000000000000000000").unwrap();
        let cipher = hex::decode("6d251e6944b051e04eaa6fb4dbf78465").unwrap();

        let c = AESBlockCipher::create(&key).unwrap();

        let mut buf = Vec::new();
        buf.resize(c.block_size(), 0);

        c.encrypt_block(&plain, &mut buf);
        assert_eq!(&buf, &cipher);

        c.decrypt_block(&cipher, &mut buf);
        assert_eq!(&buf, &plain);
    }

    #[test]
    fn aes128_2_test() {
        // Taken from AES GCM Test Case 3
        let key = hex::decode("feffe9928665731c6d6a8f9467308308").unwrap();
        let plain = hex::decode("cafebabefacedbaddecaf88800000002").unwrap();
        let cipher = hex::decode("9bb22ce7d9f372c1ee2b28722b25f206").unwrap();

        let c = AESBlockCipher::create(&key).unwrap();

        let mut buf = Vec::new();
        buf.resize(c.block_size(), 0);

        c.encrypt_block(&plain, &mut buf);
        assert_eq!(&buf, &cipher);

        c.decrypt_block(&cipher, &mut buf);
        assert_eq!(&buf, &plain);
    }

    #[test]
    fn aes256_test() {
        let key = hex::decode("984ca75f4ee8d706f46c2d98c0bf4a45f5b00d791c2dfeb191b5ed8e420fd627")
            .unwrap();
        let plain = hex::decode("00000000000000000000000000000000").unwrap();
        let cipher = hex::decode("4307456a9e67813b452e15fa8fffe398").unwrap();

        let c = AESBlockCipher::create(&key).unwrap();

        let mut buf = Vec::new();
        buf.resize(c.block_size(), 0);

        c.encrypt_block(&plain, &mut buf);
        assert_eq!(&buf, &cipher);

        c.decrypt_block(&cipher, &mut buf);
        assert_eq!(&buf, &plain);
    }
}
