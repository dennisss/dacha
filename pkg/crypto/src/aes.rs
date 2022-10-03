use alloc::vec::Vec;
#[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
use core::arch::x86_64::*;

use common::errors::*;
use math::{from_m128i, to_m128i};

use crate::aes_generic::*;
use crate::cipher::*;
use crate::utils::xor;
use crate::utils::xor_inplace;

// TODO: See also https://botan.randombit.net/doxygen/aes__ni_8cpp_source.html

const AES_BLOCK_SIZE: usize = 16;

type RoundKey = [u8; AES_BLOCK_SIZE];

const AES128_NUM_ROUNDS: usize = 10;

const AES256_NUM_ROUNDS: usize = 14;

/// Generated by running through RoundConstantIter.
const AES128_ROUND_CONSTANTS: [i32; 11] = [1, 2, 4, 8, 16, 32, 64, 128, 27, 54, 108];

// TODO: Use CLMUL https://en.wikipedia.org/wiki/CLMUL_instruction_set

// https://en.wikipedia.org/wiki/Rijndael_key_schedule#Round_constants
struct RoundConstantIter {
    last: Option<u8>,
}

impl RoundConstantIter {
    fn new() -> Self {
        Self { last: None }
    }

    /// Returns the next round constant as a 32-bit word. The 8-bit non-zero
    /// rc_i value will always be the first byte in native-endian order.
    fn next_ne_word(&mut self) -> u32 {
        if cfg!(target_endian = "big") {
            (self.next().unwrap() as u32) << 24
        } else {
            self.next().unwrap() as u32
        }
    }
}

impl core::iter::Iterator for RoundConstantIter {
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
        let round_keys_enc = Self::round_keys(key)?;
        let round_keys_dec = Self::decryption_round_keys(&round_keys_enc);

        Ok(Self {
            round_keys_enc,
            round_keys_dec,
        })
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
    fn round_keys(key: &[u8]) -> Result<Vec<RoundKey>> {
        if key.len() == 16 {
            Ok(Self::aes128_round_keys(key))
        } else if key.len() == 32 {
            Ok(Self::aes256_round_keys(key))
        } else {
            Self::round_keys_generic(key)
        }
    }

    #[cfg(not(all(target_arch = "x86_64", target_feature = "aes")))]
    fn round_keys(key: &[u8]) -> Result<Vec<RoundKey>> {
        Self::round_keys_generic(key)
    }

    /// Round keys implementation which can run on any CPU platform, but doesn't
    /// use any special AES acceleration instructions.
    fn round_keys_generic(key: &[u8]) -> Result<Vec<RoundKey>> {
        // Number of 32 bit words in the key.
        let n = key.len() / 4;

        // Number of round keys.
        let r = match key.len() * 8 {
            128 => 11,
            192 => 13,
            256 => 15,
            _ => {
                return Err(err_msg("Unsupported key length"));
            }
        };

        fn rot_word(w: u32) -> u32 {
            if cfg!(target_endian = "big") {
                w.rotate_left(8)
            } else {
                w.rotate_right(8)
            }
        }

        let mut rc_iter = RoundConstantIter::new();

        let mut words = vec![];
        for i in 0..4 * r {
            let w_i = if i < n {
                u32::from_ne_bytes(*array_ref![key, 4 * i, 4])
            } else if (i % n) == 0 {
                words[i - n] ^ sub_word(rot_word(words[i - 1])) ^ rc_iter.next_ne_word()
            } else if n > 6 && ((i % n) == 4) {
                words[i - n] ^ sub_word(words[i - 1])
            } else {
                words[i - n] ^ words[i - 1]
            };

            words.push(w_i);
        }

        // Combine words into individual round keys.
        // Aka convert from [u32; i] to [u128; i/4]
        let mut keys = vec![];
        // for i in 0..(words.len() / 4) {
        //     let mut k = RoundKey::default();
        //     (&mut k[0..4]).copy_from_slice(&words[4*i].to_ne_bytes());
        //     (&mut k[4..8]).copy_from_slice(&words[4*i+1].to_ne_bytes());
        //     (&mut k[8..12]).copy_from_slice(&words[4*i+2].to_ne_bytes());
        //     (&mut k[12..16]).copy_from_slice(&words[4*i+3].to_ne_bytes());
        //     keys.push(k);
        // }
        {
            let word_data = unsafe {
                core::slice::from_raw_parts::<RoundKey>(
                    core::mem::transmute(words.as_ptr()),
                    words.len() / 4,
                )
            };
            keys.extend_from_slice(word_data);
        }

        Ok(keys)
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
    fn aes128_round_keys(key: &[u8]) -> Vec<RoundKey> {
        let mut out = vec![];
        out.reserve_exact(AES128_NUM_ROUNDS + 1);

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

    #[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
    fn aes256_round_keys(key: &[u8]) -> Vec<RoundKey> {
        let mut out = vec![];
        out.reserve_exact(AES256_NUM_ROUNDS + 1);

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

    #[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
    fn decryption_round_keys(round_keys_enc: &[RoundKey]) -> Vec<RoundKey> {
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
    }

    #[cfg(not(all(target_arch = "x86_64", target_feature = "aes")))]
    fn decryption_round_keys(round_keys_enc: &[RoundKey]) -> Vec<RoundKey> {
        // Not used in generic implementation.
        vec![]
    }

    fn encrypt_block_generic(&self, block: &[u8], out: &mut [u8]) {
        assert_eq!(block.len(), self.block_size());
        assert_eq!(block.len(), out.len());

        let rks = &self.round_keys_enc;

        let mut state = *array_ref![block, 0, AES_BLOCK_SIZE];
        xor_inplace(&rks[0], &mut state);

        for i in 1..(rks.len() - 1) {
            sub_bytes(&mut state, false);
            shift_rows(&mut state, false);
            mix_columns(&mut state, false);
            xor_inplace(&rks[i], &mut state);
        }

        // Final round
        sub_bytes(&mut state, false);
        shift_rows(&mut state, false);
        xor_inplace(&rks[rks.len() - 1], &mut state);

        out.copy_from_slice(&state);
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
    fn encrypt_block_aesni(&self, block: &[u8], out: &mut [u8]) {
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

    fn decrypt_block_generic(&self, block: &[u8], out: &mut [u8]) {
        assert_eq!(block.len(), self.block_size());
        assert_eq!(block.len(), out.len());

        // NOTE: We use the same keys are for encryption here.
        let rks = &self.round_keys_enc;

        let mut state = *array_ref![block, 0, AES_BLOCK_SIZE];

        // Invert final round
        xor_inplace(&rks[rks.len() - 1], &mut state);
        shift_rows(&mut state, true);
        sub_bytes(&mut state, true);

        for i in (1..(rks.len() - 1)).rev() {
            xor_inplace(&rks[i], &mut state);
            mix_columns(&mut state, true);
            shift_rows(&mut state, true);
            sub_bytes(&mut state, true);
        }

        xor_inplace(&rks[0], &mut state);

        out.copy_from_slice(&state);
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
    fn decrypt_block_aesni(&self, block: &[u8], out: &mut [u8]) {
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

impl BlockCipher for AESBlockCipher {
    fn block_size(&self) -> usize {
        AES_BLOCK_SIZE
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
    fn encrypt_block(&self, block: &[u8], out: &mut [u8]) {
        self.encrypt_block_aesni(block, out);
    }

    #[cfg(not(all(target_arch = "x86_64", target_feature = "aes")))]
    fn encrypt_block(&self, block: &[u8], out: &mut [u8]) {
        self.encrypt_block_generic(block, out);
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "aes"))]
    fn decrypt_block(&self, block: &[u8], out: &mut [u8]) {
        self.decrypt_block_aesni(block, out);
    }

    #[cfg(not(all(target_arch = "x86_64", target_feature = "aes")))]
    fn decrypt_block(&self, block: &[u8], out: &mut [u8]) {
        self.decrypt_block_generic(block, out);
    }
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

struct ECBModeCipher<C: BlockCipher> {
    cipher: C,
}

impl<C: BlockCipher> ECBModeCipher<C> {
    fn new(cipher: C) -> Self {
        Self { cipher }
    }

    pub fn encrypt(&mut self, mut input: &[u8], mut output: &mut [u8]) {
        assert_eq!(input.len() % self.cipher.block_size(), 0);
        assert_eq!(input.len(), output.len());

        let block_size = self.cipher.block_size();
        let nblocks = input.len() / block_size;

        for _ in 0..nblocks {
            let input_block = next_block!(input, block_size);
            let output_block = next_block_mut!(output, block_size);

            self.cipher.encrypt_block(input_block, output_block);
        }
    }

    pub fn decrypt(&mut self, mut input: &[u8], mut output: &mut [u8]) {
        assert_eq!(input.len() % self.cipher.block_size(), 0);
        assert_eq!(input.len(), output.len());

        let block_size = self.cipher.block_size();
        let nblocks = input.len() / block_size;

        for _ in 0..nblocks {
            let input_block = next_block!(input, block_size);
            let output_block = next_block_mut!(output, block_size);

            self.cipher.decrypt_block(input_block, output_block);
        }
    }
}

// TODO: Start testing this.
struct CBCModeCipher<C: BlockCipher> {
    cipher: C,
    iv: Vec<u8>,
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

        for _ in 0..nblocks {
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

        for _ in 0..nblocks {
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
    use common::hex;

    #[test]
    fn round_constant_test() {
        let expected = [
            0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1B, 0x36, 0x6C, 0xD8, 0xAB, 0x4D,
            0x9A, 0x2F, 0x5E, 0xBC, 0x63, 0xC6, 0x97, 0x35, 0x6A, 0xD4, 0xB3, 0x7D, 0xFA, 0xEF,
            0xC5,
        ];

        let mut iter = RoundConstantIter::new();
        for c in expected {
            assert_eq!(c, iter.next().unwrap());
        }
    }

    #[test]
    fn mix_columns_test() {
        // Test vectors from
        // https://en.wikipedia.org/wiki/Rijndael_MixColumns#Test_vectors_for_MixColumn().

        let initial_state1 = [
            219, 19, 83, 69, 242, 10, 34, 92, 1, 1, 1, 1, 198, 198, 198, 198,
        ];

        let mut state1 = initial_state1.clone();

        mix_columns(&mut state1, false);
        assert_eq!(
            state1,
            [142, 77, 161, 188, 159, 220, 88, 157, 1, 1, 1, 1, 198, 198, 198, 198]
        );

        mix_columns(&mut state1, true);
        assert_eq!(state1, initial_state1);

        let initial_state2 = [
            212, 212, 212, 213, 45, 38, 49, 76, 1, 1, 1, 1, 198, 198, 198, 198,
        ];

        let mut state2 = initial_state2.clone();

        mix_columns(&mut state2, false);
        assert_eq!(
            state2,
            [213, 213, 215, 214, 77, 126, 189, 248, 1, 1, 1, 1, 198, 198, 198, 198]
        );

        mix_columns(&mut state2, true);
        assert_eq!(state2, initial_state2);
    }

    #[test]
    fn shift_rows_test() {
        let initial_state = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];

        let mut state = initial_state.clone();

        shift_rows(&mut state, false);
        assert_eq!(
            state,
            [1, 6, 11, 16, 5, 10, 15, 4, 9, 14, 3, 8, 13, 2, 7, 12]
        );

        shift_rows(&mut state, true);
        assert_eq!(state, initial_state);
    }

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

    #[async_std::test]
    async fn aes_ecb_nist_test() -> Result<()> {
        let project_dir = common::project_dir();

        let paths = &[
            "testdata/nist/aes/kat/ECBGFSbox128.rsp",
            "testdata/nist/aes/kat/ECBGFSbox256.rsp",
            "testdata/nist/aes/kat/ECBKeySbox128.rsp",
            "testdata/nist/aes/kat/ECBKeySbox256.rsp",
            "testdata/nist/aes/kat/ECBVarKey128.rsp",
            "testdata/nist/aes/kat/ECBVarKey256.rsp",
            "testdata/nist/aes/kat/ECBVarTxt128.rsp",
            "testdata/nist/aes/kat/ECBVarTxt256.rsp",
            // "testdata/nist/aes/mct/ECBMCT128.rsp",
            // "testdata/nist/aes/mct/ECBMCT256.rsp",
            "testdata/nist/aes/mmt/ECBMMT128.rsp",
            "testdata/nist/aes/mmt/ECBMMT256.rsp",
        ];

        for path in paths.iter().cloned() {
            // println!("FILE {}", path);

            let file = crate::nist::response::ResponseFile::open(project_dir.join(path)).await?;

            for response in file.iter() {
                let response = response?;

                let encrypt = response.attributes.contains_key("ENCRYPT");
                let decrypt = response.attributes.contains_key("DECRYPT");

                let key = hex::decode(response.fields.get("KEY").unwrap())?;
                let plaintext = hex::decode(response.fields.get("PLAINTEXT").unwrap())?;
                let ciphertext = hex::decode(response.fields.get("CIPHERTEXT").unwrap())?;

                let mut aes = ECBModeCipher::new(AESBlockCipher::create(&key)?);

                // println!("RUNNING {}", response.fields["COUNT"]);

                if encrypt {
                    let mut output = vec![];
                    output.resize(plaintext.len(), 0);

                    aes.encrypt(&plaintext, &mut output);

                    assert_eq!(
                        output,
                        ciphertext,
                        "{} vs {}",
                        hex::encode(&output),
                        hex::encode(&ciphertext)
                    );
                } else if decrypt {
                    let mut output = vec![];
                    output.resize(ciphertext.len(), 0);

                    aes.decrypt(&ciphertext, &mut output);

                    assert_eq!(output, plaintext);
                } else {
                    panic!("Unknown testing mode");
                }
            }
        }

        Ok(())
    }
}
