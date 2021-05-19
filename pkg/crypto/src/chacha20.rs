// Stream cipher as defined in:
// https://tools.ietf.org/html/rfc7539
// TODO: Look at the updated version in rfc8439.

use math::big::*;

// TODO: Provide some warning of when the counter will overflow and wrap.

const BLOCK_SIZE: usize = 64;
const CHACHA20_KEY_SIZE: usize = 32;
const CHACHA20_NONCE_SIZE: usize = 12;

type State = [u32; 16];

struct ChaCha20 {
    /// This will always contain the key, nonce, etc.
    /// The only part of this that should be mutated is the counter.
    state: State,

    /// Number of bytes processed up to now.
    bytes_processed: usize,
}

impl ChaCha20 {
    // Key should be 256bits.
    // Nonce should be 12 bytes
    fn new(key: &[u8], nonce: &[u8]) -> Self {
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

        for i in 0..10 {
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

    // TODO: Vectorize this
    // NOTE: a.len() should be <= b.len() as we will only xor up to the size of
    // a.
    fn xor(a: &[u8], b: &[u8], out: &mut [u8]) {
        for i in 0..a.len() {
            out[i] = a[i] ^ b[i];
        }
    }

    fn encrypt(&mut self, data: &[u8], out: &mut [u8]) {
        assert_eq!(data.len(), out.len());

        let mut i = 0;
        let n = data.len() / BLOCK_SIZE;
        while i < n * BLOCK_SIZE {
            let key_stream = self.get_block();
            let j = i + BLOCK_SIZE;
            Self::xor(&data[i..j], &key_stream, &mut out[i..j]);
            self.bytes_processed += BLOCK_SIZE;
            i = j;
        }

        let r = data.len() % BLOCK_SIZE;
        if r != 0 {
            let key_stream = self.get_block();
            i = data.len() - r;
            Self::xor(&data[i..], &key_stream, &mut out[i..]);
            self.bytes_processed += r;
        }
    }

    fn decrypt(&mut self, data: &[u8], out: &mut [u8]) {
        self.encrypt(data, out);
    }

    /// Generates a 32byte one time key to be used with poly1305
    fn poly1305_keygen(&mut self) -> Vec<u8> {
        let key_block = self.get_block_at_count(0);
        key_block[0..32].to_vec()
    }
}

struct Poly1305 {}

impl Poly1305 {
    fn new() -> Self {
        Self {}
    }

    fn tag_size(&self) -> usize {
        16
    }

    fn clamp(r: &mut [u8]) {
        r[3] &= 15;
        r[7] &= 15;
        r[11] &= 15;
        r[15] &= 15;
        r[4] &= 252;
        r[8] &= 252;
        r[12] &= 252;
    }

    fn mac(&self, data: &[u8], key: &[u8]) -> Vec<u8> {
        let p = BigUint::from(130).exp2() - &BigUint::from(5);
        assert_eq!(key.len(), 32);
        let r = {
            let mut data = (&key[0..16]).to_vec();
            Self::clamp(&mut data);
            BigUint::from_le_bytes(&data)
        };
        let s = BigUint::from_le_bytes(&key[16..]);

        let mut acc = BigUint::zero();

        for i in (0..data.len()).step_by(16) {
            let j = std::cmp::min(data.len(), i + 16);
            // TODO: Make sure that this internally reserves the full size + 1
            let mut n = BigUint::from_le_bytes(&data[i..j]);
            n.set_bit((j - i) * 8, 1);

            acc += n;
            acc = Modulo::new(&p).mul(&r, &acc);
        }

        acc += s;

        let mut out = acc.to_le_bytes();
        out.resize(16, 0);
        out
    }
}

struct ChaCha20Poly1305 {}

#[cfg(test)]
mod tests {
    use super::*;
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
        let key = common::hex::decode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
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
    fn poly1305_test() {
        let key = common::hex::decode("85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b")
            .unwrap();
        let plain = b"Cryptographic Forum Research Group";
        let tag = common::hex::decode("a8061dc1305136c6c22b8baf0c0127a9").unwrap();

        assert_eq!(&Poly1305::new().mac(&plain[..], &key), &tag);
    }
}
