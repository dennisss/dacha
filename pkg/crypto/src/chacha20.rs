// Stream cipher as defined in:
// https://tools.ietf.org/html/rfc7539
// TODO: Look at the updated version in rfc8439.

/*
Useful resources:
- https://loup-vaillant.fr/tutorials/poly1305-design
*/

use std::ops::SubAssign;

use common::errors::*;
use math::big::*;
use typenum::U320;

use crate::aead::AuthEncAD;
use crate::utils::xor;

// TODO: Provide some warning of when the counter will overflow and wrap.

pub const CHACHA20_BLOCK_SIZE: usize = 64;
pub const CHACHA20_KEY_SIZE: usize = 32;
pub const CHACHA20_NONCE_SIZE: usize = 12;

type State = [u32; 16];

pub struct ChaCha20 {
    /// This will always contain the key, nonce, etc.
    /// The only part of this that should be mutated is the counter.
    state: State,

    /// Number of bytes processed up to now.
    bytes_processed: usize,
}

impl ChaCha20 {
    /// Key should be 256bits.
    /// Nonce should be 12 bytes
    pub fn new(key: &[u8], nonce: &[u8]) -> Self {
        assert_eq!(key.len(), CHACHA20_KEY_SIZE);
        assert_eq!(nonce.len(), CHACHA20_NONCE_SIZE);

        let mut state = [0u32; 16];
        state[0] = 0x61707865;
        state[1] = 0x3320646e;
        state[2] = 0x79622d32;
        state[3] = 0x6b206574;

        for i in 0..8 {
            state[4 + i] = u32::from_le_bytes(*array_ref![key, 4 * i, 4]);
        }

        // Counter for the last block (so the first block will use 1).
        state[12] = 0;

        for i in 0..3 {
            state[13 + i] = u32::from_le_bytes(*array_ref![nonce, 4 * i, 4]);
        }

        Self {
            state,
            bytes_processed: 0,
        }
    }

    fn quarter_round_with(mut a: u32, mut b: u32, mut c: u32, mut d: u32) -> (u32, u32, u32, u32) {
        // 1.
        a = a.wrapping_add(b);
        d ^= a;
        d = d.rotate_left(16);

        // 2.
        c = c.wrapping_add(d);
        b ^= c;
        b = b.rotate_left(12);

        // 3.
        a = a.wrapping_add(b);
        d ^= a;
        d = d.rotate_left(8);

        // 4.
        c = c.wrapping_add(d);
        b ^= c;
        b = b.rotate_left(7);

        (a, b, c, d)
    }

    fn quarter_round(state: &mut State, x: usize, y: usize, z: usize, w: usize) {
        let (a, b, c, d) = Self::quarter_round_with(state[x], state[y], state[z], state[w]);

        state[x] = a;
        state[y] = b;
        state[z] = c;
        state[w] = d;
    }

    fn serialize(state: State) -> [u8; 64] {
        // TODO: On a little endian system, this should be simple transmute

        let mut buf = [0u8; 64];
        for i in 0..state.len() {
            buf[(4 * i)..(4 * (i + 1))].copy_from_slice(&state[i].to_le_bytes());
        }

        buf
    }

    fn get_block(&mut self) -> [u8; 64] {
        let i: usize = 1 + (self.bytes_processed / 64);
        // We should never reuse the same counter value.
        assert!(i <= u32::max_value() as usize);
        self.get_block_at_count(i as u32)
    }

    fn get_block_at_count(&mut self, counter: u32) -> [u8; 64] {
        // Set the counter.
        self.state[12] = counter;

        let mut state = self.state.clone();

        for _ in 0..10 {
            Self::quarter_round(&mut state, 0, 4, 8, 12);
            Self::quarter_round(&mut state, 1, 5, 9, 13);
            Self::quarter_round(&mut state, 2, 6, 10, 14);
            Self::quarter_round(&mut state, 3, 7, 11, 15);
            Self::quarter_round(&mut state, 0, 5, 10, 15);
            Self::quarter_round(&mut state, 1, 6, 11, 12);
            Self::quarter_round(&mut state, 2, 7, 8, 13);
            Self::quarter_round(&mut state, 3, 4, 9, 14);
        }

        for i in 0..state.len() {
            state[i] = state[i].wrapping_add(self.state[i]);
        }

        Self::serialize(state)
    }

    pub fn encrypt(&mut self, data: &[u8], out: &mut [u8]) {
        assert_eq!(data.len(), out.len());

        let mut i = 0;
        let n = data.len() / CHACHA20_BLOCK_SIZE;
        while i < n * CHACHA20_BLOCK_SIZE {
            let key_stream = self.get_block();
            let j = i + CHACHA20_BLOCK_SIZE;
            xor(&data[i..j], &key_stream, &mut out[i..j]);
            self.bytes_processed += CHACHA20_BLOCK_SIZE;
            i = j;
        }

        let r = data.len() % CHACHA20_BLOCK_SIZE;
        if r != 0 {
            let key_stream = self.get_block();
            i = data.len() - r;
            xor(&data[i..], &key_stream, &mut out[i..]);
            self.bytes_processed += r;
        }
    }

    pub fn decrypt(&mut self, data: &[u8], out: &mut [u8]) {
        self.encrypt(data, out);
    }

    /// Generates a 32byte one time key to be used with poly1305
    pub fn poly1305_keygen(&mut self) -> Vec<u8> {
        let key_block = self.get_block_at_count(0);
        key_block[0..32].to_vec()
    }
}

/// The modulus we use is 130 bits which fits in 5 32-bit numbers.
/// We need twice that to support multiplication.
type Poly1305Uint = SecureBigUint<U320>;

/*
Need the prime

prime'

Split


*/

/*
/// Using the variable naming from https://en.wikipedia.org/wiki/Montgomery_modular_multiplication.
struct MontgomeryModulo {
    modulus: Poly1305Uint,
    aux_modulus: Poly1305Uint,
    aux_modulus_mask: Poly1305Uint,
    aux_modulus_log2: usize,

    modulus_inv: Poly1305Uint,
    aux_modulus_inv: Poly1305Uint,
}

impl MontgomeryModulo {
    /// Converts a number
    fn to_montgomery_form(&self, value: &Poly1305Uint) -> Poly1305Uint {
        let mut temp = Poly1305Uint::zero();
        self.aux_modulus.mul_to(value, &mut temp);

        let (_, r) = temp.quorem(&self.modulus);
        r
    }

    // Basically zero out all upper bits.

    fn mod_r(&self, value: &Poly1305Uint) -> Poly1305Uint {
        // TOOD: Usually don't need to mask every single byte.
        let mut value = value.clone();
        value.and_assign(&self.aux_modulus_mask);
        value
    }

    fn from_montgomery_form(&self, value: &Poly1305Uint) -> Poly1305Uint {
        // m = (T mod R) N' mod R
        let m = self.mod_r(&self.mod_r(&value).mul(&self.modulus_inv));

        // t = (T + m N) / R
        let mut t = value.add(&m.mul(&self.modulus)).shr(self.aux_modulus_log2);

        let zero = Poly1305Uint::zero();

        let sub = if t >= self.modulus {
            &self.modulus
        } else {
            &zero
        };
        t.sub_assign(&sub);

        t
    }
}

fn to_secure(val: &BigUint) -> Poly1305Uint {
    let data = val.to_le_bytes();
    Poly1305Uint::from_le_bytes(&data)
}
*/

/// Optimized integer for storing at least 130-bit integers while performing
/// operations modulo the prime '2^130 - 5'.
#[derive(Clone, Copy)]
struct U1305 {
    /// Each contains 26-bits of the integer. In little-endian order.
    limbs: [u64; 5],
}

impl U1305 {
    pub fn zero() -> Self {
        Self { limbs: [0; 5] }
    }

    /// Returns '2^130 - 5'
    pub fn modulus() -> Self {
        const MAX_LIMB: u64 = (1 << 26) - 1;
        Self {
            limbs: [MAX_LIMB - 4, MAX_LIMB, MAX_LIMB, MAX_LIMB, MAX_LIMB],
        }
    }

    pub fn from(value: u16) -> Self {
        let mut v = Self::zero();
        v.limbs[0] = value as u64;
        v
    }

    pub fn from_le_bytes(data: &[u8; 16]) -> Self {
        let v0 = u32::from_le_bytes(*array_ref![data, 0, 4]) & ((1 << 26) - 1);
        // Start at 26
        let v1 = (u32::from_le_bytes(*array_ref![data, 3, 4]) >> 2) & ((1 << 26) - 1);
        // Start at 52
        let v2 = (u32::from_le_bytes(*array_ref![data, 6, 4]) >> 4) & ((1 << 26) - 1);
        let v3 = (u32::from_le_bytes(*array_ref![data, 9, 4]) >> 6) & ((1 << 26) - 1);
        let v4 = {
            let mut buf = [0u8; 4];
            buf[0..3].copy_from_slice(array_ref![data, 13, 3]);
            u32::from_le_bytes(buf) & ((1 << 26) - 1)
        };

        return Self {
            limbs: [v0 as u64, v1 as u64, v2 as u64, v3 as u64, v4 as u64],
        };

        /*
        let mut out = Self::zero();

        let mut data_i = 0;
        let mut acc = 0;
        let mut acc_bits = 0;

        for i in 0..out.limbs.len() {
            while acc_bits < 26 && data_i < data.len() {
                acc |= (data[data_i] as u64) << acc_bits;
                data_i += 1;
                acc_bits += 8;
            }

            out.limbs[i] = acc & ((1 << 26) - 1);
            acc >>= 26;
            acc_bits -= 26;
        }

        out
        */
    }

    /// NOTE: Only preserved the least significant 16 bytes.
    pub fn to_le_bytes(&self) -> [u8; 16] {
        let mut data = [0u8; 16];

        let mut limb_i = 0;
        let mut acc = 0;
        let mut acc_bits = 0;

        for i in 0..data.len() {
            while acc_bits < 8 && limb_i < self.limbs.len() {
                acc |= self.limbs[limb_i] << acc_bits;
                acc_bits += 26;
                limb_i += 1;
            }

            data[i] = acc as u8;
            acc >>= 8;
            acc_bits -= 8;
        }

        data
    }

    pub fn add(&mut self, rhs: &Self) {
        let mut carry = 0;
        for i in 0..self.limbs.len() {
            let v = carry + self.limbs[i] + rhs.limbs[i];

            self.limbs[i] = v & ((1 << 26) - 1);
            carry = v >> 26;
        }

        // Put the carry into the last limb.
        self.limbs[4] |= carry << 26;
    }

    pub fn add_mod_n(&mut self, rhs: &Self) {
        self.add(rhs);
        self.maybe_sub_modulus();
    }

    pub fn add_pow2(&mut self, pow2: usize) {
        let byte = pow2 / 26;
        let shift = pow2 % 26;
        self.limbs[byte] |= 1 << shift;
    }

    pub fn sub(&mut self, rhs: &Self) {
        let mut carry = 0;
        let n = self.limbs.len();
        for i in 0..n {
            // TODO: Try to use overflowing_sub instead (that way we don't need to go to
            // 64bits)
            let v = (self.limbs[i] as i64) - (rhs.limbs[i] as i64) + carry;
            let offset;
            if v < 0 {
                offset = 1 << 26;
                carry = -1;
            } else {
                offset = 0;
                carry = 0;
            }

            self.limbs[i] = (v + offset) as u64;
        }

        debug_assert_eq!(carry, 0);
    }

    pub fn greater_eq(&self, rhs: &Self) -> bool {
        let mut carry = 0;
        for i in 0..self.limbs.len() {
            let v = (self.limbs[i] as i64) - (rhs.limbs[i] as i64) + carry;
            if v < 0 {
                carry = -1;
            } else {
                carry = 0;
            }
        }

        carry == 0
    }

    fn maybe_sub_modulus(&mut self) {
        let sub;
        if self.greater_eq(&Self::modulus()) {
            sub = Self::modulus();
        } else {
            sub = Self::zero();
        }
        self.sub(&sub);
    }

    fn mul_mod_n(&self, rhs: &Self) -> Self {
        let mut out = [0u64; 5];

        // TODO: Unroll me.
        for i in 0..self.limbs.len() {
            for j in 0..rhs.limbs.len() {
                let mut k = i + j;
                let mut v = self.limbs[i] * rhs.limbs[j];
                if k >= self.limbs.len() {
                    k -= self.limbs.len();
                    v *= 5;
                }

                out[k] += v;
            }
        }

        // Perform carry propgation.
        // 5 is 3 bits long, so worst case we overflow by 3 bits.
        let mut carry = 0;
        for _ in 0..3 {
            for i in 0..out.len() {
                let v = out[i] + carry;
                out[i] = v & ((1 << 26) - 1);
                carry = v >> 26;
            }

            carry *= 5;
        }

        debug_assert_eq!(carry, 0);

        let mut out = Self { limbs: out };
        out.maybe_sub_modulus();

        // debug_assert!(out < Self::modulus());

        out
    }
}

/*
impl std::cmp::Ord for U1305 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let mut less = 0;
        let mut greater = 0;

        for i in (0..self.limbs.len()).rev() {
            let mask = !(less | greater);

            if self.limbs[i] < other.limbs[i] {
                less |= mask & 1;
            } else if self.limbs[i] > other.limbs[i] {
                greater |= mask & 1;
            }
        }

        let cmp = (less << 1) | greater;

        let mut out = std::cmp::Ordering::Equal;
        // Exactly one of these if statements should always be triggered.
        if cmp == 0b10 {
            out = std::cmp::Ordering::Less;
        }
        if cmp == 0b01 {
            out = std::cmp::Ordering::Greater;
        }
        if cmp == 0b00 {
            out = std::cmp::Ordering::Equal;
        }

        out
    }
}

impl std::cmp::PartialEq for U1305 {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl std::cmp::Eq for U1305 {}

impl std::cmp::PartialOrd for U1305 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
*/

// TOOD: Need to switch to a static allocation / constant time implementation.
#[derive(Clone)]
pub struct Poly1305 {
    // modulo: MontgomeryModulo,

    // // Constant prime number used as modulus.
    // p: Poly1305Uint,

    // Integer form of the first half of the key.
    r: U1305,

    /// Integer form of the second half of the key.
    s: U1305,

    /// Current accumulator value.
    acc: U1305,

    zero: U1305,
    temp: U1305,
}

impl Poly1305 {
    pub fn new(key: &[u8]) -> Self {
        // let modulo = {
        //     let p = BigUint::from(130).exp2() - BigUint::from(5);
        //     // Auxiliary modulus that is coprime to the main modulus.
        //     let r = BigUint::from(130).exp2();

        //     let p_inv = Modulo::new(&r).inv(&p);
        //     let r_inv = Modulo::new(&p).inv(&r);

        //     MontgomeryModulo {
        //         modulus: to_secure(&p),
        //         modulus_inv: to_secure(&p_inv),
        //         aux_modulus_mask: to_secure(&(&r - &BigUint::from(1))),
        //         aux_modulus_log2: 130,
        //         aux_modulus: to_secure(&r),
        //         aux_modulus_inv: to_secure(&r_inv),
        //     }
        // };

        assert_eq!(key.len(), 32);
        Self {
            // modulo,
            // p: {
            //     let mut v = Poly1305Uint::from(130).exp2();
            //     v.sub_assign(&Poly1305Uint::from(5));
            //     v
            // },
            r: {
                let mut data = *array_ref![key, 0, 16];
                Self::clamp(&mut data);

                // NOTE: The upper bits of this are cleared during the clamp so this will
                // already by reduced modulo the prime.
                U1305::from_le_bytes(&data)
            },
            s: U1305::from_le_bytes(array_ref![key, 16, 16]),
            acc: U1305::zero(),

            zero: U1305::zero(),
            temp: U1305::zero(),
        }
    }

    fn tag_size(&self) -> usize {
        16
    }

    fn clamp(r: &mut [u8]) {
        r[3] &= 15;

        r[4] &= 252;
        r[7] &= 15;

        r[8] &= 252;
        r[11] &= 15;

        r[12] &= 252;
        r[15] &= 15;
    }

    /*
    r:   Keep always in montgomery form.
    acc: Easy to put initially into montgomery form.

    Adding to it is easy.

    Eventually use REDC to take out of montgomery form.

    - Split input into 28-bit chunks (in 32-bit registers.)
    - Multiply into a 64-bit wide size.
        -

    May have caries from the last column meaning that we need to

    */

    /*

    Core cheat is that 2^130 mod N is 5

    Decompose the number into a part < 2^130

    ( x = i * 2^130 + j ) mod N

    x = i * 5 + j mod N


    */

    // a R mod N
    // R is a power of 2

    /// NOTE: Data will be interprated as if it were padded with zeros to a
    /// multiple of 16 bytes.
    pub fn update(&mut self, data: &[u8], pad_to_block: bool) {
        let mut buffer = [0u8; 16];

        for i in (0..data.len()).step_by(16) {
            let j = std::cmp::min(data.len(), i + 16);

            let buf = {
                if i + 16 <= data.len() {
                    array_ref![data, i, 16]
                } else {
                    buffer[0..(j - i)].copy_from_slice(&data[i..]);
                    &buffer
                }
            };

            // buf[0..(j - i)].copy_from_slice(&data[i..j]);

            let mut n = U1305::from_le_bytes(buf);

            let final_bit = if pad_to_block { 16 * 8 } else { (j - i) * 8 };
            n.add_pow2(final_bit); // NOTE: Will never cause it go over the modulus.

            self.acc.add_mod_n(&n);
            self.acc = self.acc.mul_mod_n(&self.r);
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        // TODO: This doesn't need to do any mod operations.
        self.acc.add(&self.s);
        self.acc.to_le_bytes().to_vec()
    }
}

// fn map_blocks<F: FnMut(&Block)>(data: &[u8], mut f: F) {
//     let n = data.len() / 16;
//     let r = data.len() % 16;

//     for i in 0..n {
//         f(array_ref![data, BLOCK_SIZE * i, BLOCK_SIZE]);
//     }

//     if r != 0 {
//         let mut block = [0u8; BLOCK_SIZE];
//         block[0..r].copy_from_slice(&data[(data.len() - r)..]);
//         f(&block);
//     }
// }

#[derive(Clone)]
pub struct ChaCha20Poly1305 {}

impl ChaCha20Poly1305 {
    pub fn new() -> Self {
        Self {}
    }

    fn compute_tag(otk: Vec<u8>, ciphertext: &[u8], additional_data: &[u8]) -> Vec<u8> {
        let mut poly = Poly1305::new(&otk);

        poly.update(additional_data, true);
        poly.update(ciphertext, true);

        let mut lengths = [0u8; 16];
        (&mut lengths[0..8]).copy_from_slice(&(additional_data.len() as u64).to_le_bytes());
        (&mut lengths[8..16]).copy_from_slice(&(ciphertext.len() as u64).to_le_bytes());
        poly.update(&lengths, false);

        poly.finish()
    }
}

impl AuthEncAD for ChaCha20Poly1305 {
    fn key_size(&self) -> usize {
        CHACHA20_KEY_SIZE
    }

    fn nonce_range(&self) -> (usize, usize) {
        (CHACHA20_NONCE_SIZE, CHACHA20_NONCE_SIZE)
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
        let start_i = out.len();
        out.resize(start_i + self.expanded_size(plaintext.len()), 0);

        let (ciphertext, tag) = out.split_at_mut(plaintext.len());

        let mut chacha = ChaCha20::new(key, nonce);
        let otk = chacha.poly1305_keygen();

        chacha.encrypt(plaintext, ciphertext);

        tag.copy_from_slice(&Self::compute_tag(otk, ciphertext, additional_data));
    }

    fn decrypt(
        &self,
        key: &[u8],
        nonce: &[u8],
        ciphertext: &[u8],
        additional_data: &[u8],
        out: &mut Vec<u8>,
    ) -> Result<()> {
        let (ciphertext, tag) = ciphertext.split_at(ciphertext.len() - 16);

        let start_i = out.len();
        out.resize(start_i + ciphertext.len(), 0);
        let plaintext = &mut out[start_i..];

        let mut chacha = ChaCha20::new(key, nonce);
        let otk = chacha.poly1305_keygen();
        chacha.decrypt(ciphertext, plaintext);

        let expected_tag = Self::compute_tag(otk, ciphertext, additional_data);

        if !crate::constant_eq(tag, &expected_tag) {
            return Err(err_msg("Bad tag"));
        }

        Ok(())
    }

    fn box_clone(&self) -> Box<dyn AuthEncAD> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test::*;
    use std::str::FromStr;

    #[test]
    fn chacha20_quarter_round_test() {
        let (a, b, c, d) =
            ChaCha20::quarter_round_with(0x11111111, 0x01020304, 0x9b8d6f43, 0x01234567);

        assert_eq!(a, 0xea2a92f4);
        assert_eq!(b, 0xcb1cf8ce);
        assert_eq!(c, 0x4581472e);
        assert_eq!(d, 0x5881c4bb);
    }

    #[test]
    fn chacha20_encrypt_test() {
        let key =
            common::hex::decode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
                .unwrap();
        let nonce = common::hex::decode("000000000000004a00000000").unwrap();

        let plain = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

        let cipher = common::hex::decode("6e2e359a2568f98041ba0728dd0d6981e97e7aec1d4360c20a27afccfd9fae0bf91b65c5524733ab8f593dabcd62b3571639d624e65152ab8f530c359f0861d807ca0dbf500d6a6156a38e088a22b65e52bc514d16ccf806818ce91ab77937365af90bbf74a35be6b40b8eedf2785e42874d").unwrap();

        let mut out = vec![];
        out.resize(plain.len(), 0);

        let mut c = ChaCha20::new(&key, &nonce);
        c.encrypt(&plain[..], &mut out);
        assert_eq!(&out, &cipher);

        let mut c2 = ChaCha20::new(&key, &nonce);
        c2.decrypt(&cipher[..], &mut out);
        assert_eq!(&out[..], &plain[..]);
    }

    #[test]
    fn chacha20_poly1305_keygen_test() {
        let key =
            common::hex::decode("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
                .unwrap();
        let nonce = common::hex::decode("000000000001020304050607").unwrap();

        let expected =
            common::hex::decode("8ad5a08b905f81cc815040274ab29471a833b637e3fd0da508dbb8e2fdd1a646")
                .unwrap();

        let mut chacha = ChaCha20::new(&key, &nonce);
        let otk = chacha.poly1305_keygen();

        assert_eq!(otk, expected);
    }

    /*
    #[test]
    fn u1305_test() {
        let mut zero = U1305::zero();
        zero.add_pow2(16 * 8);

        let mut a = U1305::from_le_bytes(&[0xff, 0xff]);
        assert_eq!(
            &a.to_le_bytes(),
            &[0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );

        let b = U1305::from_le_bytes(&[0x00, 0x00, 0xff, 0xff]);
        assert_eq!(
            &b.to_le_bytes(),
            &[0, 0, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );

        a.add_mod_n(&b);

        assert_eq!(
            &a.to_le_bytes(),
            &[0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );

        // assert!(a > b);
        // assert!(a == a);
        // assert!(b == b);

        let seven = U1305::from(7);
        let hundred = U1305::from(100);
        let seven_hundren = seven.mul_mod_n(&hundred);
        assert_eq!(&seven_hundren.limbs, &[700, 0, 0, 0, 0]);

        let mut five = U1305::from(20);
        five.add_mod_n(&U1305::modulus());
        assert_eq!(&five.limbs, &[20, 0, 0, 0, 0]);

        // 2^128
        let big1 = U1305::from_le_bytes(
            &BigUint::from_str("340282366920938463463374607431768211456")
                .unwrap()
                .to_le_bytes(),
        );

        let big2 = big1.mul_mod_n(&big1);

        // (2^128 * 2^128) mod N
        let expected = BigUint::from_str("425352958651173079329218259289710264320")
            .unwrap()
            .to_le_bytes();
        assert_eq!(&big2.to_le_bytes(), &expected[0..16]);

        let max = U1305::from_le_bytes(&[0xff; 17]);
        let out = max.mul_mod_n(&max);
        assert_eq!(&out.limbs, &[16, 0, 0, 0, 0]);
    }
    */

    #[test]
    fn poly1305_test() {
        let key =
            common::hex::decode("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b")
                .unwrap();
        let plain = b"Cryptographic Forum Research Group";
        let tag = common::hex::decode("a8061dc1305136c6c22b8baf0c0127a9").unwrap();

        let mut poly = Poly1305::new(&key);
        poly.update(&plain[..], false);

        assert_eq!(&poly.finish(), &tag);
    }

    #[test]
    fn aead_test() {
        let plaintext = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

        let aad = common::hex::decode("50515253c0c1c2c3c4c5c6c7").unwrap();

        let key =
            common::hex::decode("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
                .unwrap();

        // 32-bit constant | iv
        let nonce = common::hex::decode("070000004041424344454647").unwrap();

        let ciphertext = common::hex::decode("d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d63dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b3692ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc3ff4def08e4b7a9de576d26586cec64b61161ae10b594f09e26a7e902ecbd0600691").unwrap();

        let aead = ChaCha20Poly1305::new();

        let mut out = vec![];
        aead.encrypt(&key, &nonce, plaintext, &aad, &mut out);
        assert_eq!(&out, &ciphertext);

        let mut out2 = vec![];
        let r = aead.decrypt(&key, &nonce, &ciphertext, &aad, &mut out2);
        assert!(r.is_ok());
        assert_eq!(out2, plaintext);

        // TODO: Also test decryption failures.
    }

    #[test]
    fn poly1305_leak_test() {
        let key_inputs = typical_boundary_buffers(32);

        // TODO: Switch back to 32
        // TODO: If the number is too small, we may notice the 'Clone' performance
        // rather than that of the algorithm.
        let data_inputs = typical_boundary_buffers(4096);

        println!("Poly1305::new() timing:");
        TimingLeakTest::new(
            key_inputs.iter(),
            |key| {
                Poly1305::new(key);
                true
            },
            TimingLeakTestOptions {
                num_iterations: 100000,
            },
        )
        .run();

        println!("Poly1305::update() timing:");

        let mut test_cases: Vec<(Poly1305, &[u8])> = vec![];
        for key in &key_inputs {
            for data in &data_inputs {
                test_cases.push((Poly1305::new(key), data));
            }
        }

        TimingLeakTest::new(
            test_cases.iter(),
            |(poly, data)| {
                let mut p = poly.clone();
                p.update(data, false);
                p.finish()[0] > 1
            },
            TimingLeakTestOptions {
                num_iterations: 100000,
            },
        )
        .run();

        // TODO: Do performance tests on a flat chunk of data without cloning
        // the poly instance.

        // TODO: Test the finish method.
        /*
        key: 32 bytes

        data: sizes from 1-32 bytes
        pad_block either true or false.
        */

        // Need:
        // - Test case descriptor type 'D'
        // - Instantiated test case type 'T'
        // - Test case generator to go from 'D' to 'T'
        // - Function to run the code under test using 'T'

        // Test cases:
        // - Test cae
        // - Define a set of test cases and a way to generate each of them.
    }
}
