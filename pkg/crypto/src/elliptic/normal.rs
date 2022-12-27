use alloc::boxed::Box;
use alloc::vec::Vec;

use asn::encoding::{DERReadable, DERWriteable};
use common::errors::*;
use math::big::*;
use math::Integer;

use crate::dh::DiffieHellmanFn;
use crate::hasher::Hasher;
use crate::random::secure_random_range;

/// Parameters of an elliptic curve of the form:
/// y^2 = x^3 + a*x + b
///
/// aka, a curve in short Weierstrass form.
#[derive(PartialEq, Debug, Clone)]
pub struct EllipticCurve {
    pub a: SecureBigUint,
    pub b: SecureBigUint,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct EllipticCurvePoint {
    pub x: SecureBigUint,
    pub y: SecureBigUint,
    pub inf: bool,
}

impl EllipticCurvePoint {
    /// Checks if this point is the infinity point.
    pub fn is_inf(&self) -> bool {
        self.inf
    }

    /// Creates an infinity/identity/zero point.
    ///
    /// The special characteristics of this is that any point P:
    /// P + I = P
    pub fn inf(bit_width: usize) -> Self {
        Self {
            x: SecureBigUint::from_usize(0, bit_width),
            y: SecureBigUint::from_usize(0, bit_width),
            inf: true,
        }
    }

    pub fn copy_if(&self, should_copy: bool, out: &mut Self) {
        self.x.copy_if(should_copy, &mut out.x);
        self.y.copy_if(should_copy, &mut out.y);

        // TODO: Ensure constant time.
        out.inf = (self.inf & should_copy) | (out.inf & !should_copy)
    }
}

// TODO: Test this is constant time.
fn swap_bools_if(a: &mut bool, b: &mut bool, should_swap: bool) {
    let filter = should_swap & (*a ^ *b);
    *a ^= filter;
    *b ^= filter;
}

/// Parameters for a group of points on an elliptic curve definited over a
/// finite field of integers.
///
/// All points are multiples of a base point 'g' modulo a prime 'p'.
#[derive(PartialEq, Debug, Clone)]
pub struct EllipticCurveGroup {
    /// Base curve.
    curve: EllipticCurve,

    /// Prime number which is the size of the finite field (all curve points are
    /// calculated 'mod p').
    p: SecureBigUint,

    /// Base point on the curve.
    g: EllipticCurvePoint,

    /// Multiplicative order of the curve.
    /// Also a prime number.
    ///
    /// NOTE: May be larger or smaller than 'p'
    n: SecureBigUint,

    /// Cofactor
    k: usize,
}

#[async_trait]
impl DiffieHellmanFn for EllipticCurveGroup {
    /// Generates a secret value.
    async fn secret_value(&self) -> Result<Vec<u8>> {
        assert!(self.k == 1);
        let two = SecureBigUint::from_usize(2, 32);
        let n = secure_random_range(&two, &self.n).await?;

        // NOTE: The length is bound by 'n', not 'p'.
        Ok(n.to_be_bytes())
    }

    fn public_value(&self, secret: &[u8]) -> Result<Vec<u8>> {
        // TODO: Check that this is correct for usage in TLS.

        let sk = self.decode_scalar(secret)?;

        // Compute 'public_point = secret_scalar * base_point'.
        let p = self.scalar_mul_base_point(&sk);

        if p.is_inf() {
            return Err(err_msg("Bad secret value resulted in infinite point"));
        }

        Ok(self.encode_point(&p))
    }

    // TODO: Must match FE2OSP definition.
    fn shared_secret(&self, remote_public: &[u8], local_secret: &[u8]) -> Result<Vec<u8>> {
        let mut p = self.decode_point(remote_public)?;
        let s = self.decode_scalar(local_secret)?;

        // Computes 'shared_secret_point = local_secret_scalar * remote_public_value'.
        // We only retain the 'x' coordinate of the resulting point as this is the only
        // part used in TLS.
        let v_x = self.scalar_mul_point(&s, &p).x;

        // Will be left padded up to the size of 'p'.
        Ok(v_x.to_be_bytes())
    }
}

impl EllipticCurveGroup {
    /*
    Note that the private_key is a random integer d_a in the range [1, n).
    Public key is the curve point 'd_a * G'
    (same as diffi-hellman secret_value() and public_value())
    */

    pub(super) fn from_bytes(
        p_str: &[u8],
        a_str: &[u8],
        b_str: &[u8],
        g_x_str: &[u8],
        g_y_str: &[u8],
        n_str: &[u8],
        h: usize,
    ) -> Self {
        // TODO: Flip to native ordering using a macro.
        let p = SecureBigUint::from_be_bytes(p_str);
        let a = SecureBigUint::from_be_bytes(a_str);
        let b = SecureBigUint::from_be_bytes(b_str);
        let g_x = SecureBigUint::from_be_bytes(g_x_str);
        let g_y = SecureBigUint::from_be_bytes(g_y_str);
        let n = SecureBigUint::from_be_bytes(n_str);

        EllipticCurveGroup {
            curve: EllipticCurve { a, b },
            p,
            g: EllipticCurvePoint {
                x: g_x,
                y: g_y,
                inf: false,
            },
            n,
            k: h,
        }
    }

    /// See https://en.wikipedia.org/wiki/Elliptic_Curve_Digital_Signature_Algorithm.
    pub async fn create_signature(
        &self,
        private_key: &[u8],
        data: &[u8],
        hasher: &mut dyn Hasher,
    ) -> Result<Vec<u8>> {
        let digest = hasher.finish_with(data);

        let two = SecureBigUint::from_usize(2, 32);

        for _ in 0..4 {
            let k = secure_random_range(&two, &self.n).await?;
            if let Some(val) = self.create_signature_with(private_key, &digest, &k)? {
                return Ok(val);
            }
        }

        Err(err_msg("Exhausted tried to make a signature"))
    }

    pub fn create_signature_with(
        &self,
        private_key: &[u8],
        digest: &[u8],
        random: &SecureBigUint,
    ) -> Result<Option<Vec<u8>>> {
        let mut d_a = self.decode_scalar(private_key)?;

        /// Length of 'z' in bits (same as 'n').
        /// TODO: Once SecureBigUint supports storing a partial number of bits,
        /// use bit_width() here.
        let z_length = self.n.value_bits(); // NOTE: 'n' is publicly known.
        if z_length > 8 * digest.len() {
            return Err(err_msg("Message digest too short"));
        }

        // z_length leftmost bits of digest ('mod n')
        let mut z = {
            let mut v = SecureBigUint::from_be_bytes(digest);
            v.shr_n(8 * digest.len() - z_length);
            v.truncate(self.n.bit_width());
            v.reduce_once(&self.n);
            v
        };

        /// x_1 = (random_scalar*base_point).x
        let x_1 = self.scalar_mul_base_point(random).x;

        /// NOTE: x_1 was computed 'mod p' where 'p' may be much larger than
        /// 'n'. TODO: When n is only somewhat smaller, use barett
        /// reduction
        let r = SecureModulo::new(&self.n).rem(&x_1);

        if r.is_zero() {
            return Ok(None);
        }

        // s = k^-1 (z + r d_a) mod n
        let s = {
            // TODO: Have a wrapper function that gurantees that the numbers passed to
            // modulo are already reduced.
            let modulo = SecureMontgomeryModulo::new(&self.n);

            let mut random = random.clone();
            let mut r = r.clone();

            modulo.to_montgomery_form(&mut random);
            modulo.to_montgomery_form(&mut r);
            modulo.to_montgomery_form(&mut z);
            modulo.to_montgomery_form(&mut d_a);

            // TODO: Given we are doing so few multiplications here, it is probably more
            // effiicent to use baret reduction.
            let s = modulo.mul(
                &modulo.inv_prime_mod(&random),
                &modulo.add(&z, &modulo.mul(&r, &d_a)),
            );

            modulo.from_montgomery_form(&s)
        };

        if s.is_zero() {
            return Ok(None);
        }

        let sig = pkix::PKIX1Algorithms2008::ECDSA_Sig_Value {
            r: BigUint::from_le_bytes(&r.to_le_bytes()).into(),
            s: BigUint::from_le_bytes(&s.to_le_bytes()).into(),
        };

        Ok(Some(sig.to_der()))
    }

    // ECDSA
    pub fn verify_signature(
        &self,
        public_key: &[u8],
        signature: &[u8],
        data: &[u8],
        hasher: &mut dyn Hasher,
    ) -> Result<bool> {
        hasher.update(data);
        let digest = hasher.finish();
        self.verify_digest_signature(public_key, signature, &digest)
    }

    /// TODO: Consider offering a non-constant time version of there when it is
    /// not important to avoid leaking the message.
    pub fn verify_digest_signature(
        &self,
        public_key: &[u8],
        signature: &[u8],
        digest: &[u8],
    ) -> Result<bool> {
        // TODO: We should allow passing in an Into<Bytes> to avoid cloning the
        // data here.
        let (r, s) = {
            let parsed = pkix::PKIX1Algorithms2008::ECDSA_Sig_Value::from_der(signature.into())?;

            let mut r = SecureBigUint::from_le_bytes(&parsed.r.to_uint()?.to_le_bytes());
            let mut s = SecureBigUint::from_le_bytes(&parsed.s.to_uint()?.to_le_bytes());

            // Both be in the range [1, n).
            let one = SecureBigUint::from_usize(1, 32);
            if r < one || r >= self.n || s < one || s >= self.n {
                return Err(err_msg("Signature out of range"));
            }

            // NOTE: ASN.1 integers are stored using a minimum number of bytes.
            // This should not panic as we already verified that the numbers aren't larger
            // than the modulus.
            r.extend(self.n.bit_width());
            s.extend(self.n.bit_width());

            (r, s)
        };

        /// Length of 'z' in bits (same as 'n').
        let z_length = self.n.value_bits(); // NOTE: 'n' is publicly known.
        if z_length > 8 * digest.len() {
            return Err(err_msg("Message digest too short"));
        }

        // z_length leftmost bits of digest
        let z = {
            let mut v = SecureBigUint::from_be_bytes(digest);
            v.shr_n(8 * digest.len() - z_length);
            v.truncate(self.n.bit_width());
            v.reduce_once(&self.n);
            v
        };

        // u_1 = z s^-1 mod n
        // u_2 = r s^-1 mod n
        let (u_1, u_2) = {
            let m = SecureMontgomeryModulo::new(&self.n);

            let mut r = r.clone();
            let mut s = s.clone();
            let mut z = z.clone();
            m.to_montgomery_form(&mut r);
            m.to_montgomery_form(&mut s);
            m.to_montgomery_form(&mut z);

            let s_inv = m.inv_prime_mod(&s);

            (
                m.from_montgomery_form(&m.mul(&z, &s_inv)),
                m.from_montgomery_form(&m.mul(&r, &s_inv)),
            )
        };

        // TODO: Validate that public_key != n x point = identity?
        // Also check that n &* public_key = identity.
        let point = self.decode_point(public_key)?;

        // output_point = u_1 G + u_2 point
        let output_point = {
            let m = SecureMontgomeryModulo::new(&self.p);

            let mut g = self.g.clone();
            let mut point = point.clone();

            m.to_montgomery_form(&mut g.x);
            m.to_montgomery_form(&mut g.y);
            m.to_montgomery_form(&mut point.x);
            m.to_montgomery_form(&mut point.y);

            let a = self.scalar_mul_point_impl(&u_1, &g, &m);
            let b = self.scalar_mul_point_impl(&u_2, &point, &m);
            let c = self.add_points(&a, &b, &m);

            EllipticCurvePoint {
                x: m.from_montgomery_form(&c.x),
                y: m.from_montgomery_form(&c.y),
                inf: c.inf,
            }
        };

        // Valid if 'r mod n === output_point.x mod n'
        // TODO: Use modular equivalence
        let mut modulo = SecureModulo::new(&self.n);
        Ok(modulo.rem(&r) == modulo.rem(&output_point.x))
    }

    fn decode_scalar(&self, data: &[u8]) -> Result<SecureBigUint> {
        // TODO: Check this against the spec (why is the proper length of the private
        // key?)
        if data.len() != self.n.byte_width() {
            return Err(format_err!(
                "Scalar wrong size: {} vs {}",
                data.len(),
                self.n.byte_width()
            ));
        }

        let v = SecureBigUint::from_be_bytes(data);
        if v >= self.n {
            return Err(err_msg("Scalar larger than group order"));
        }

        Ok(v)
    }

    fn decode_point(&self, data: &[u8]) -> Result<EllipticCurvePoint> {
        if data.len() <= 1 {
            return Err(err_msg("Point too small"));
        }

        let nbytes = self.p.byte_width();
        let x1 = if data[0] == 4 {
            // Uncompressed form
            // TODO: For TLS 1.3, this is the only supported format
            if data.len() != 1 + 2 * nbytes {
                return Err(format_err!(
                    "Point data too small: {} vs {}",
                    data.len(),
                    1 + 2 * nbytes
                ));
            }

            let x = SecureBigUint::from_be_bytes(&data[1..(nbytes + 1)]);
            let y = SecureBigUint::from_be_bytes(&data[(nbytes + 1)..]);

            EllipticCurvePoint { x, y, inf: false }
        } else if data[0] == 2 || data[0] == 3 {
            // Compressed form.
            // Contains only X, data[0] contains the LSB of Y.
            if data.len() != 1 + nbytes {
                return Err(err_msg("Point data too small"));
            }

            return Err(err_msg("Compressed point format not supported"));

            /*
            // TODO: Off by one
            let x = SecureBigUint::from_be_bytes(&data[1..nbytes]);

            // Compute y^2 from the x.
            let y2 = (&x).pow(&3.into()) + &(&self.curve.a * &x) + &self.curve.b;

            // NOTE: We do not check that y*y == y^2 as this will be checked
            // by verify_point anyway.
            let mut y = y2.isqrt();

            // There are always two square roots, so make sure we got the right
            // one.
            let lsb = data[0] & 0b1;
            if lsb != (y.bit(0) as u8) {
                // TODO: For ECDSA should this use the other modulus?
                y = Modulo::new(&self.p).negate(&y);
            }

            EllipticCurvePoint { x, y }
            */
        } else {
            return Err(format_err!("Unknown point format {}", data[0]));
        };

        let p = x1;

        if !self.verify_point(&p) {
            return Err(err_msg("Invalid point"));
        }

        Ok(p)
    }

    fn encode_point(&self, p: &EllipticCurvePoint) -> Vec<u8> {
        let mut out = vec![];
        out.push(4); // Uncompressed form

        out.extend_from_slice(&p.x.to_be_bytes());
        out.extend_from_slice(&p.y.to_be_bytes());
        out
    }

    /// Assuming that p != q, this computes 'p + q' in the curve group.
    ///
    /// This is only valid if both 'p' and 'q' are not at infinity.
    ///
    /// NOTE: 'p' and 'q' should already be in Montgomery form.
    ///
    /// The equations used for this and double_point are described in:
    /// https://en.wikipedia.org/wiki/Elliptic_curve_point_multiplication#Point_addition
    fn add_points(
        &self,
        p: &EllipticCurvePoint,
        q: &EllipticCurvePoint,
        m: &SecureMontgomeryModulo,
    ) -> EllipticCurvePoint {
        // slope = (y_q - y_p) / (x_q - x_p)
        let slope = m.mul(&m.sub(&q.y, &p.y), &m.inv_prime_mod(&m.sub(&q.x, &p.x)));
        self.intersecting_point(p, q, &slope, m)
    }

    /// NOTE: 'p' should already be in Montgomery form.
    fn double_point(
        &self,
        p: &EllipticCurvePoint,
        m: &SecureMontgomeryModulo,
    ) -> EllipticCurvePoint {
        // TODO: Instead just use addition rather than multiplying by these small
        // constants.
        let mut two = SecureBigUint::from_usize(2, self.p.bit_width());
        let mut three = SecureBigUint::from_usize(3, self.p.bit_width());
        m.to_montgomery_form(&mut two);
        m.to_montgomery_form(&mut three);

        let mut a = self.curve.a.clone();
        m.to_montgomery_form(&mut a);

        // slope = (3 x_P^2 + a) / (2 y_P)
        let slope = m.mul(
            &m.add(&m.mul(&three, &m.mul(&p.x, &p.x)), &a),
            &m.inv_prime_mod(&m.mul(&two, &p.y)),
        );

        self.intersecting_point(p, p, &slope, m)
    }

    /// Internal shared logic of the above two methods.
    fn intersecting_point(
        &self,
        p: &EllipticCurvePoint,
        q: &EllipticCurvePoint,
        slope: &SecureBigUint,
        m: &SecureMontgomeryModulo,
    ) -> EllipticCurvePoint {
        // x_R = slope^2 - (x_P + x_Q)
        let x = m.sub(&m.mul(slope, slope), &m.add(&p.x, &q.x));

        // y_R = slope*(x_P - x_R) - y_P
        let y = m.sub(&m.mul(slope, &m.sub(&p.x, &x)), &p.y);

        let mut out = EllipticCurvePoint { x, y, inf: false };

        p.copy_if(q.is_inf(), &mut out);
        q.copy_if(p.is_inf(), &mut out);

        out
    }

    /// Multiplies an arbitrary point with a scalar.
    ///
    /// Internally uses the montgomery ladder approach.
    ///
    /// NOTE: The input 'p' should be in Montgomery form while the input scalar
    /// 'd' MUST be in normal form.
    fn scalar_mul_point_impl(
        &self,
        d: &SecureBigUint,
        p: &EllipticCurvePoint,
        m: &SecureMontgomeryModulo,
    ) -> EllipticCurvePoint {
        let mut r_0 = EllipticCurvePoint::inf(self.p.bit_width());

        // NOTE: 'p' passed in as montgomery form.
        let mut r_1 = p.clone();

        let mut swap = false;

        for i in (0..d.bit_width()).rev() {
            let d_i = d.bit(i) != 0;
            swap ^= d_i;

            r_0.x.swap_if(&mut r_1.x, swap);
            r_0.y.swap_if(&mut r_1.y, swap);
            swap_bools_if(&mut r_0.inf, &mut r_1.inf, swap);
            swap = d_i;

            r_1 = self.add_points(&r_0, &r_1, m);
            r_0 = self.double_point(&r_0, m);
        }

        r_0.x.swap_if(&mut r_1.x, swap);
        r_0.y.swap_if(&mut r_1.y, swap);
        swap_bools_if(&mut r_0.inf, &mut r_1.inf, swap);

        r_0
    }

    /// Returns whether or not the given point is on the curve.
    /// TODO: There is also a 'point on curve' verification algorithm here:
    /// https://en.wikipedia.org/wiki/Elliptic_Curve_Digital_Signature_Algorithm#Signature_verification_algorithm
    ///
    /// TODO: Should this be constant time?
    pub fn verify_point(&self, p: &EllipticCurvePoint) -> bool {
        // Must not be at infinity
        if p.is_inf() {
            return false;
        }

        // Must be within the 'mod p' field.
        if p.x >= self.p || p.y >= self.p {
            return false;
        }

        // Must be on the curve.

        let mut m = SecureModulo::new(&self.p);

        // TODO: Major speed up opportunities if we use barett style reduction.

        // y^2
        let lhs = m.mul(&p.y, &p.y);

        // x^3 + a*x + b
        let rhs = {
            m.add(
                &m.pow(&p.x, &SecureBigUint::from_usize(3, 32)),
                &m.add(&m.mul(&self.curve.a, &p.x), &self.curve.b),
            )
        };

        // NOTE: Both are reduced by the modulus.
        lhs == rhs
    }

    /// Multiples the given point 'p' by itself 'd' times.
    pub fn scalar_mul_point(
        &self,
        d: &SecureBigUint,
        p: &EllipticCurvePoint,
    ) -> EllipticCurvePoint {
        let modulo = SecureMontgomeryModulo::new(&self.p);

        let mut p = p.clone();
        modulo.to_montgomery_form(&mut p.x);
        modulo.to_montgomery_form(&mut p.y);

        let p = self.scalar_mul_point_impl(d, &p, &modulo);

        EllipticCurvePoint {
            x: modulo.from_montgomery_form(&p.x),
            y: modulo.from_montgomery_form(&p.y),
            inf: p.inf,
        }
    }

    /// Multiplies the base curve point by itself 'd' times.
    pub fn scalar_mul_base_point(&self, d: &SecureBigUint) -> EllipticCurvePoint {
        let mut g = self.g.clone();

        let modulo = SecureMontgomeryModulo::new(&self.p);
        // TODO: Precompute this.
        modulo.to_montgomery_form(&mut g.x);
        modulo.to_montgomery_form(&mut g.y);

        let p = self.scalar_mul_point_impl(d, &g, &modulo);

        EllipticCurvePoint {
            x: modulo.from_montgomery_form(&p.x),
            y: modulo.from_montgomery_form(&p.y),
            inf: p.inf,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asn::encoding::DERWriteable;
    use std::str::FromStr;

    #[test]
    fn small_elliptic_curve_test() {
        fn big(v: usize) -> SecureBigUint {
            SecureBigUint::from_usize(v, 32)
        }

        let k = big(2);
        let x = big(80);
        let y = big(10);

        let ecc = EllipticCurveGroup {
            curve: EllipticCurve {
                a: big(2),
                b: big(3),
            },
            p: big(97),
            g: EllipticCurvePoint {
                x: big(3),
                y: big(6),
                inf: false,
            },
            n: big(100),
            k: 1,
        };

        let out = ecc.scalar_mul_base_point(&k);
        assert_eq!(out.x, x);
        assert_eq!(out.y, y);
    }

    #[test]
    fn secp256r1_test() {
        let k = SecureBigUint::from_str(
            "29852220098221261079183923314599206100666902414330245206392788703677545185283",
            256,
        )
        .unwrap();
        let x = SecureBigUint::from_be_bytes(&hex!(
            "9EACE8F4B071E677C5350B02F2BB2B384AAE89D58AA72CA97A170572E0FB222F"
        ));
        let y = SecureBigUint::from_be_bytes(&hex!(
            "1BBDAEC2430B09B93F7CB08678636CE12EAAFD58390699B5FD2F6E1188FC2A78"
        ));

        let ecc = EllipticCurveGroup::secp256r1();

        let out = ecc.scalar_mul_base_point(&k);

        assert_eq!(out.x, x);
        assert_eq!(out.y, y);
    }

    #[testcase]
    async fn encoding_point_sizes() -> Result<()> {
        // In RFC 8446 Section 4.2.8.2, the size of the points is well defined.

        let mut test_cases = vec![
            (EllipticCurveGroup::secp256r1(), 1 + 2 * 32),
            (EllipticCurveGroup::secp384r1(), 1 + 2 * 48),
            (EllipticCurveGroup::secp521r1(), 1 + 2 * 66),
        ];

        for (curve, expected_size) in test_cases {
            let secret = curve.secret_value().await?;
            let public_value = curve.public_value(&secret)?;
            assert_eq!(public_value.len(), expected_size);
        }

        Ok(())
    }

    #[test]
    fn ecdsa_test() -> Result<()> {
        // Test vectors grabbed from:
        // https://github.com/bcgit/bc-java/blob/master/core/src/test/java/org/bouncycastle/crypto/test/ECTest.java#L384
        // testECDSASecP224k1sha256

        let curve = EllipticCurveGroup::secp224k1();

        let private_key = hex!("00000000BE6F6E91FE96840A6518B56F3FE21689903A64FA729057AB872A9F51");
        let random = hex!("00c39beac93db21c3266084429eb9b846b787c094f23a4de66447efbb3");

        // TODO: This is the message post digestion.
        let digest = hex!("E5D5A7ADF73C5476FAEE93A2C76CE94DC0557DB04CDC189504779117920B896D");
        let r = BigUint::from_be_bytes(&hex!(
            "8163E5941BED41DA441B33E653C632A55A110893133351E20CE7CB75"
        ));
        let s = BigUint::from_be_bytes(&hex!(
            "D12C3FC289DDD5F6890DCE26B65792C8C50E68BF551D617D47DF15A8"
        ));

        let sig = pkix::PKIX1Algorithms2008::ECDSA_Sig_Value {
            r: r.into(),
            s: s.into(),
        }
        .to_der();

        let new_sig = curve
            .create_signature_with(
                &private_key,
                &digest,
                &SecureBigUint::from_be_bytes(&random),
            )?
            .unwrap();

        assert_eq!(new_sig, sig);

        let point = hex!("04C5C9B38D3603FCCD6994CBB9594E152B658721E483669BB42728520F484B537647EC816E58A8284D3B89DFEDB173AFDC214ECA95A836FA7C");

        // let mut hasher = crate::sha256::SHA256Hasher::default();

        assert!(curve
            .verify_digest_signature(&point, &sig, &digest)
            .unwrap());

        Ok(())
    }

    #[testcase]
    async fn ecdsa_sign_test() -> Result<()> {
        let group = EllipticCurveGroup::secp256r1();

        let secret = group.secret_value().await?;
        let data = b"hello world";

        let mut hasher = crate::sha256::SHA256Hasher::default();

        let signature = group.create_signature(&secret, data, &mut hasher).await?;

        let mut hasher2 = crate::sha256::SHA256Hasher::default();

        let public = group.public_value(&secret)?;

        assert!(group.verify_signature(&public, &signature, data, &mut hasher2)?);

        Ok(())
    }
}
