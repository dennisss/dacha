use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec::Vec;
use core::marker::PhantomData;

use common::ceil_div;
use common::errors::*;
use common::hex;
use math::big::*;
use math::Integer;

use crate::dh::DiffieHellmanFn;
use crate::random::secure_random_bytes;

/// Closed group of points on a Montgomery curve of the form:
/// 'v^2 = u^3 + A*u^2 + u'.
///
/// All points are defined as a scalar multiple of a well known base point.
///
/// All numbers are defined in GF(p) for a prime number 'p'.
pub struct MontgomeryCurveGroup<C: MontgomeryCurveCodec> {
    /// Prime number used as the modulus for all operations on the curve.
    p: SecureBigUint,

    /// U coordinate of the base point.
    u: SecureBigUint,

    /// Number of bits
    bits: usize,

    /// (A - 2) / 4
    a24: SecureBigUint,
    codec: PhantomData<C>,
}

impl<C: MontgomeryCurveCodec> MontgomeryCurveGroup<C> {
    fn new(p: SecureBigUint, u: SecureBigUint, bits: usize, a24: SecureBigUint) -> Self {
        Self {
            p,
            u,
            bits,
            a24,
            codec: PhantomData,
        }
    }

    /// Multiplies a curve point with the given 'u' coordinate by itself 'k'
    /// times.
    fn mul(&self, k: &SecureBigUint, u: &SecureBigUint) -> SecureBigUint {
        curve_mul(k, u, &self.p, self.bits, &self.a24)
    }
}

impl MontgomeryCurveGroup<X25519> {
    pub fn x25519() -> Self {
        let bits = 255;
        let p = {
            let working_bits = 256;
            let mut v = SecureBigUint::exp2(255, working_bits)
                - SecureBigUint::from_usize(19, working_bits);
            v.truncate(bits);
            v
        };
        let u = SecureBigUint::from_usize(9, bits);
        let a24 = SecureBigUint::from_usize(121665, bits);
        Self::new(p, u, bits, a24)
    }
}

impl MontgomeryCurveGroup<X448> {
    pub fn x448() -> Self {
        let bits = 448;
        let p = {
            let working_bits = 449;
            let mut v = SecureBigUint::exp2(448, working_bits)
                - SecureBigUint::exp2(224, working_bits)
                - SecureBigUint::from_usize(1, working_bits);
            v.truncate(bits);
            v
        };
        let u = SecureBigUint::from_usize(5, bits);
        let a24 = SecureBigUint::from_usize(39081, bits);
        Self::new(p, u, bits, a24)
    }
}

#[async_trait]
impl<C: MontgomeryCurveCodec + Send + Sync> DiffieHellmanFn for MontgomeryCurveGroup<C> {
    /// Creates a new fixed length private key
    /// This will be the 'k'/scalar used to multiple the base point.
    async fn secret_value(&self) -> Result<Vec<u8>> {
        let mut data = vec![];
        // TODO: Get the key length from the codec?
        data.resize(ceil_div(self.bits, 8), 0);
        secure_random_bytes(&mut data).await?;
        Ok(data)
    }

    // TODO: Implement the result routes

    /// Generates the public key associated with the given private key.
    fn public_value(&self, secret: &[u8]) -> Result<Vec<u8>> {
        let k = C::decode_scalar(secret);
        // Multiply secret*base_point
        let out = self.mul(&k, &self.u);
        Ok(C::encode_u_cord(&out))
    }

    fn shared_secret(&self, remote_public: &[u8], local_secret: &[u8]) -> Result<Vec<u8>> {
        // NOTE: The RFC specified that we should accept out of range values as their
        // 'mod p' equivalent.
        let mut u = C::decode_u_cord(remote_public);
        u.reduce_once(&self.p); // TODO: Check this is sufficient reduction.

        let k = C::decode_scalar(local_secret);

        let out = self.mul(&k, &u);
        Ok(C::encode_u_cord(&out))

        // TODO: Validate shared secret is not all zero
        // ^ See https://tools.ietf.org/html/rfc7748#section-6.1 for how to do it securely
    }
}

pub trait MontgomeryCurveCodec: Send + Sync {
    // NOTE: There is generally no need for encoding the scalar.
    fn decode_scalar(data: &[u8]) -> SecureBigUint;

    fn encode_u_cord(u: &SecureBigUint) -> Vec<u8>;
    fn decode_u_cord(data: &[u8]) -> SecureBigUint;
}

pub struct X25519 {}

impl MontgomeryCurveCodec for X25519 {
    // tODO: All

    // TODO: Implement unit tests to ensure this is never >= 'p'
    fn decode_scalar(data: &[u8]) -> SecureBigUint {
        // TODO: Return an error instead.
        assert_eq!(data.len(), 32);

        let mut sdata = data.to_vec();
        sdata[0] &= 248;
        sdata[31] &= 127;
        sdata[31] |= 64;

        SecureBigUint::from_le_bytes(&sdata)
    }

    // TODO: Must assert that it is 32 bytes and error out if it isn't.
    fn decode_u_cord(data: &[u8]) -> SecureBigUint {
        assert_eq!(data.len(), 32);

        let mut sdata = data.to_vec();
        // Mask MSB in last byte (only applicable to X25519).
        sdata[31] &= 0x7f;

        SecureBigUint::from_le_bytes(&sdata)
    }

    fn encode_u_cord(u: &SecureBigUint) -> Vec<u8> {
        let mut data = u.to_le_bytes();
        assert!(data.len() <= 32);
        data.resize(32, 0);
        data
    }
}

pub struct X448 {}

impl MontgomeryCurveCodec for X448 {
    fn decode_scalar(data: &[u8]) -> SecureBigUint {
        // TODO: This is not enough bytes for our integers.
        assert_eq!(data.len(), 56);

        let mut sdata = data.to_vec();
        sdata[0] &= 252;
        sdata[55] |= 128;

        SecureBigUint::from_le_bytes(&sdata)
    }

    fn decode_u_cord(data: &[u8]) -> SecureBigUint {
        assert_eq!(data.len(), 56);
        SecureBigUint::from_le_bytes(data)
    }

    fn encode_u_cord(u: &SecureBigUint) -> Vec<u8> {
        let mut data = u.to_le_bytes();
        assert!(data.len() == 56); // <- Will always be fixed length with secure ints.
        data
    }
}

/// Multiplies the curve point with the given 'u' coordinate by itself 'k'
/// times.
///
/// From RFC 7748
fn curve_mul(
    k: &SecureBigUint,
    u: &SecureBigUint,
    p: &SecureBigUint,
    bits: usize,
    a24: &SecureBigUint,
) -> SecureBigUint {
    // TODO: Allow SecureMontgomeryModulo to do everything so we don't need two
    // separate instances here. Also have a ModuloWrapped class to ensure all
    // numbers are wrapped.
    let modulo = SecureMontgomeryModulo::new(p);

    let mut x_1 = u.clone();
    let mut x_2 = SecureBigUint::from_usize(1, p.bit_width());
    let mut z_2 = SecureBigUint::from_usize(0, p.bit_width());
    let mut x_3 = u.clone();
    let mut z_3 = SecureBigUint::from_usize(1, p.bit_width());

    let mut a24_mont = a24.clone();

    modulo.to_montgomery_form(&mut x_1);
    modulo.to_montgomery_form(&mut x_2);
    modulo.to_montgomery_form(&mut z_2);
    modulo.to_montgomery_form(&mut x_3);
    modulo.to_montgomery_form(&mut z_3);
    modulo.to_montgomery_form(&mut a24_mont);

    let mut swap = false;

    for t in (0..bits).rev() {
        let k_t = k.bit(t) != 0;
        swap ^= k_t;

        x_2.swap_if(&mut x_3, swap);
        z_2.swap_if(&mut z_3, swap);
        swap = k_t;

        let A = modulo.add(&x_2, &z_2);
        let AA = modulo.mul(&A, &A);
        let B = modulo.sub(&x_2, &z_2);

        let BB = modulo.mul(&B, &B); // B.pow(&2.into());
        let E = modulo.sub(&AA, &BB);
        let C = modulo.add(&x_3, &z_3);
        let D = modulo.sub(&x_3, &z_3);
        let DA = modulo.mul(&D, &A);
        let CB = modulo.mul(&C, &B);
        x_3 = {
            let tmp = modulo.add(&DA, &CB);
            modulo.mul(&tmp, &tmp)
        };
        // TODO: Here we can do a subtraction without cloning by taking ownership
        z_3 = modulo.mul(&x_1, {
            let tmp = modulo.sub(&DA, &CB);
            &modulo.mul(&tmp, &tmp)

            // &modulo.sub(&DA, &CB).pow(&two)
        });
        x_2 = modulo.mul(&AA, &BB);
        z_2 = modulo.mul(&E, &{
            // AA + (a24 * E)
            let tmp = modulo.mul(&a24_mont, &E);
            let tmp2 = modulo.add(&AA, &tmp);
            tmp2
        });
    }

    x_2.swap_if(&mut x_3, swap);
    z_2.swap_if(&mut z_3, swap);

    let res = modulo.mul(&x_2, &modulo.inv_prime_mod(&z_2));

    modulo.from_montgomery_form(&res)
}

#[cfg(test)]
mod tests {
    use super::*;

    // TODO: Check we have all test vectors from https://www.rfc-editor.org/rfc/rfc7748#section-5.2

    #[test]
    fn x25519_test() {
        /*
        assert_eq!(
            SecureBigUint::from_le_bytes(&hex!("01")).to_string(),
            "1"
        );
        assert_eq!(
            SecureBigUint::from_le_bytes(&hex!("0100000002")).to_string(),
            "8589934593"
        );
        */

        let scalar = SecureBigUint::from_str(
            "31029842492115040904895560451863089656472772604678260265531221036453811406496",
            256,
        )
        .unwrap();
        let u_in = SecureBigUint::from_str(
            "34426434033919594451155107781188821651316167215306631574996226621102155684838",
            256,
        )
        .unwrap();

        let u_out = MontgomeryCurveGroup::x25519().mul(&scalar, &u_in);
        assert_eq!(
            hex::encode(u_out.to_le_bytes()),
            "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552"
        );

        let scalar2 = X25519::decode_scalar(&hex!(
            "4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d"
        ));
        assert_eq!(
            scalar2.to_string(),
            "35156891815674817266734212754503633747128614016119564763269015315466259359304"
        );

        let u_in2 = X25519::decode_u_cord(&hex!(
            "e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493"
        ));
        assert_eq!(
            u_in2.to_string(),
            "8883857351183929894090759386610649319417338800022198945255395922347792736741"
        );

        let u_out2 = MontgomeryCurveGroup::x25519().mul(&scalar2, &u_in2);
        assert_eq!(
            &X25519::encode_u_cord(&u_out2),
            &hex!("95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957")
        );
    }

    #[test]
    fn ecdh_x25519_codec_test() {
        assert_eq!(
            X25519::decode_scalar(&hex!(
                "a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4"
            ))
            .to_string(),
            "31029842492115040904895560451863089656472772604678260265531221036453811406496"
        );

        assert_eq!(
            X25519::decode_u_cord(&hex!(
                "e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c"
            ))
            .to_string(),
            "34426434033919594451155107781188821651316167215306631574996226621102155684838"
        );
    }

    #[test]
    fn ecdh_x25519_test() {
        let alice_private =
            hex!("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a");
        let alice_public = hex!("8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a");
        let bob_private = hex!("5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb");
        let bob_public = hex!("de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f");
        let shared_secret =
            hex!("4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742");

        let group = MontgomeryCurveGroup::x25519();

        assert_eq!(&group.public_value(&alice_private).unwrap(), &alice_public);
        assert_eq!(&group.public_value(&bob_private).unwrap(), &bob_public);
        assert_eq!(
            &group.shared_secret(&alice_public, &bob_private).unwrap(),
            &shared_secret
        );
        assert_eq!(
            &group.shared_secret(&bob_public, &alice_private).unwrap(),
            &shared_secret
        );
    }
}
