use alloc::boxed::Box;
use std::marker::PhantomData;
use std::string::ToString;
use std::vec::Vec;

use asn::encoding::DERWriteable;
use common::ceil_div;
use common::errors::*;
use common::hex;
use common::LeftPad;
use math::big::*;
use math::integer::Integer;
use math::number::{One, Zero};

use crate::dh::*;
use crate::hasher::Hasher;
use crate::random::*;

// TODO: REALLY NEED tests with having more invalid signatures.

// TODO: Need precomputation of the montgomery modulus information for all
// curves.

// TODO: A lot of this code assumes that the prime used is divisible by the limb
// base used in SecureBigUint so that to_be_bytes doesn't add too many bytes.

/// Parameters of an elliptic curve of the form:
/// y^2 = x^3 + a*x + b
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

use asn::encoding::DERReadable;

impl EllipticCurveGroup {
    /*
    Note that the private_key is a random integer d_a in the range [1, n).
    Public key is the curve point 'd_a * G'
    (same as diffi-hellman secret_value() and public_value())
    */

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

    fn from_bytes(
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

    pub fn secp192r1() -> Self {
        Self::from_bytes(
            &hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFFFFFFFFFF"),
            &hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFFFFFFFFFC"),
            &hex!("64210519E59C80E70FA7E9AB72243049FEB8DEECC146B9B1"),
            &hex!("188DA80EB03090F67CBF20EB43A18800F4FF0AFD82FF1012"),
            &hex!("07192B95FFC8DA78631011ED6B24CDD573F977A11E794811"),
            &hex!("FFFFFFFFFFFFFFFFFFFFFFFF99DEF836146BC9B1B4D22831"),
            1,
        )
    }

    pub fn secp224k1() -> Self {
        // TODO: Use macros to compress this in the binary.
        // (or store these in a separate file)
        Self::from_bytes(
            &hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFE56D"),
            &hex!("00000000000000000000000000000000000000000000000000000000"),
            &hex!("00000000000000000000000000000000000000000000000000000005"),
            &hex!("A1455B334DF099DF30FC28A169A467E9E47075A90F7E650EB6B7A45C"),
            &hex!("7E089FED7FBA344282CAFBD6F7E319F7C0B0BD59E2CA4BDB556D61A5"),
            &hex!("010000000000000000000000000001DCE8D2EC6184CAF0A971769FB1F7"),
            1,
        )
    }

    pub fn secp224r1() -> Self {
        Self::from_bytes(
            &hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF000000000000000000000001"),
            &hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFFFFFFFFFFFFFFFFFE"),
            &hex!("B4050A850C04B3ABF54132565044B0B7D7BFD8BA270B39432355FFB4"),
            &hex!("B70E0CBD6BB4BF7F321390B94A03C1D356C21122343280D6115C1D21"),
            &hex!("BD376388B5F723FB4C22DFE6CD4375A05A07476444D5819985007E34"),
            &hex!("FFFFFFFFFFFFFFFFFFFFFFFFFFFF16A2E0B8F03E13DD29455C5C2A3D"),
            1,
        )
    }

    pub fn secp256r1() -> Self {
        Self::from_bytes(
            &hex!("FFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF"),
            &hex!("FFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFC"),
            &hex!("5AC635D8AA3A93E7B3EBBD55769886BC651D06B0CC53B0F63BCE3C3E27D2604B"),
            &hex!("6B17D1F2E12C4247F8BCE6E563A440F277037D812DEB33A0F4A13945D898C296"),
            &hex!("4FE342E2FE1A7F9B8EE7EB4A7C0F9E162BCE33576B315ECECBB6406837BF51F5"),
            &hex!("FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551"),
            1,
        )
    }

    pub fn secp384r1() -> Self {
        Self::from_bytes(
            &hex!(
                "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFF\
			 FFFFFF0000000000000000FFFFFFFF"
            ),
            &hex!(
                "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFF\
			 FFFFFF0000000000000000FFFFFFFC"
            ),
            &hex!(
                "B3312FA7E23EE7E4988E056BE3F82D19181D9C6EFE8141120314088F5013875AC6\
			 56398D8A2ED19D2A85C8EDD3EC2AEF"
            ),
            &hex!(
                "AA87CA22BE8B05378EB1C71EF320AD746E1D3B628BA79B9859F741E082542A3855\
			 02F25DBF55296C3A545E3872760AB7"
            ),
            &hex!(
                "3617DE4A96262C6F5D9E98BF9292DC29F8F41DBD289A147CE9DA3113B5F0B8C00A\
			 60B1CE1D7E819D7A431D7C90EA0E5F"
            ),
            &hex!(
                "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC7634D81F4372DDF58\
			 1A0DB248B0A77AECEC196ACCC52973"
            ),
            1,
        )
    }

    pub fn secp521r1() -> Self {
        Self::from_bytes(
            &hex!(
                "01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\
			 FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\
			 "
            ),
            &hex!(
                "01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\
			 FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC\
			 "
            ),
            &hex!(
                "0051953EB9618E1C9A1F929A21A0B68540EEA2DA725B99B315F3B8B489918EF109\
			 E156193951EC7E937B1652C0BD3BB1BF073573DF883D2C34F1EF451FD46B503F00\
			 "
            ),
            &hex!(
                "00C6858E06B70404E9CD9E3ECB662395B4429C648139053FB521F828AF606B4D3D\
			 BAA14B5E77EFE75928FE1DC127A2FFA8DE3348B3C1856A429BF97E7E31C2E5BD66\
			 "
            ),
            &hex!(
                "011839296A789A3BC0045C8A5FB42C7D1BD998F54449579B446817AFBD17273E66\
			 2C97EE72995EF42640C550B9013FAD0761353C7086A272C24088BE94769FD16650\
			 "
            ),
            &hex!(
                "01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\
			 FA51868783BF2F966B7FCC0148F709A5D03BB5C9B8899C47AEBB6FB71E91386409\
			 "
            ),
            1,
        )
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

struct EllipticCurveMultiplier {}

// TODO: Don't forget to mask the last bit when decoding k or u?

// TODO: Now all of these are using modular arithmetic right now

// TODO: See https://en.wikipedia.org/wiki/Exponentiation_by_squaring#Montgomery's_ladder_technique as a method of doing power operations in constant time

// TODO: See start of https://tools.ietf.org/html/rfc7748#section-5 for the encode/decode functions

// pub fn decode_u_cord()

// TODO: Need a custom function for sqr (aka n^2)

// 32 bytes for X25519 and 56 bytes for X448

// See also https://www.iacr.org/cryptodb/archive/2006/PKC/3351/3351.pdf

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

    #[async_std::test]
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

    #[async_std::test]
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
