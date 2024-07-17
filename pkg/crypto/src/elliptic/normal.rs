use alloc::boxed::Box;
use alloc::vec::Vec;

use asn::encoding::{DERReadable, DERWriteable};
use common::ceil_div;
use common::errors::*;
use math::big::*;
use math::Integer;

use crate::dh::DiffieHellmanFn;
use crate::hasher::Hasher;
use crate::random::SharedRng;

pub type DefaultStorage = <HeapAllocator as Allocator<'static>>::Storage;

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
            x: SecureBigUint::from_usize(0, bit_width, &HeapAllocator {}),
            y: SecureBigUint::from_usize(0, bit_width, &HeapAllocator {}),
            inf: true,
        }
    }

    pub fn copy_if(&self, should_copy: bool, out: &mut Self) {
        self.x.copy_if(should_copy, &mut out.x);
        self.y.copy_if(should_copy, &mut out.y);

        // TODO: Ensure constant time.
        out.inf = (self.inf & should_copy) | (out.inf & !should_copy)
    }

    pub fn swap_if(&mut self, should_swap: bool, other: &mut Self) {
        self.x.swap_if(&mut other.x, should_swap);
        self.y.swap_if(&mut other.y, should_swap);
        swap_bools_if(&mut self.inf, &mut other.inf, should_swap);
    }
}

// TODO: Test this is constant time.
fn swap_bools_if(a: &mut bool, b: &mut bool, should_swap: bool) {
    let filter = should_swap & (*a ^ *b);
    *a ^= filter;
    *b ^= filter;
}

/// Elliptic curve point (X, Y, Z) which makes to the normal above cordinates
/// as:
///
/// x = X / Z
/// y = Y / Z
pub struct EllipticCurveProjectivePoint {
    pub x: SecureBigUint,
    pub y: SecureBigUint,
    pub z: SecureBigUint,
    pub inf: bool,
}

impl EllipticCurveProjectivePoint {
    fn from_affine(pt: EllipticCurvePoint, m: &SecureMontgomeryModulo) -> Self {
        let mut allocator = HeapAllocator {};

        let mut z = SecureBigUint::from_usize(1, m.modulus().bit_width(), &mut allocator);
        m.to_montgomery_form(&mut z, &mut allocator);

        Self {
            x: pt.x.clone(),
            y: pt.y.clone(),
            z,
            inf: pt.inf,
        }
    }

    fn to_affine(&self, m: &SecureMontgomeryModulo) -> EllipticCurvePoint {
        let mut allocator = HeapAllocator {};
        let z_inv = m.inv_public_prime_mod(&self.z, &mut allocator);

        EllipticCurvePoint {
            x: m.mul(&self.x, &z_inv, &mut allocator),
            y: m.mul(&self.y, &z_inv, &mut allocator),
            inf: self.inf,
        }
    }

    pub fn copy_if(&self, should_copy: bool, out: &mut Self) {
        self.x.copy_if(should_copy, &mut out.x);
        self.y.copy_if(should_copy, &mut out.y);
        self.z.copy_if(should_copy, &mut out.z);
        // TODO: Ensure constant time.
        out.inf = (self.inf & should_copy) | (out.inf & !should_copy);
    }

    pub fn swap_if(&mut self, should_swap: bool, other: &mut Self) {
        self.x.swap_if(&mut other.x, should_swap);
        self.y.swap_if(&mut other.y, should_swap);
        self.z.swap_if(&mut other.z, should_swap);
        swap_bools_if(&mut self.inf, &mut other.inf, should_swap);
    }

    // See
    // https://hyperelliptic.org/EFD/g1p/auto-shortw-projective.html
    //
    // "dbl-2007-bl"

    // NOTE: All inputs should already be in montgomery form.
    fn double(&self, curve_a: &SecureBigUint, m: &SecureMontgomeryModulo) -> Self {
        let mut allocator = HeapAllocator {};

        // XX = X1^2
        let xx = m.mul(&self.x, &self.x, &mut allocator);

        // ZZ = Z1^2
        let zz = m.mul(&self.z, &self.z, &mut allocator);

        // w = a*ZZ+3*XX
        let w = {
            // t0 = 3*XX
            let t0 = m.add(&xx, &xx, &mut allocator);
            let t0 = m.add(&t0, &xx, &mut allocator);

            // t1 = a*ZZ
            let t1 = m.mul(&curve_a, &zz, &mut allocator);

            // w = t1+t0
            m.add(&t1, &t0, &mut allocator)
        };

        // s = 2*Y1*Z1
        let s = {
            // t2 = Y1*Z1
            let tmp = m.mul(&self.y, &self.z, &mut allocator);
            // s = 2*t2
            m.add(&tmp, &tmp, &mut allocator)
        };

        // ss = s^2
        let ss = m.mul(&s, &s, &mut allocator);

        // sss = s*ss
        let sss = m.mul(&s, &ss, &mut allocator);

        // R = Y1*s
        let r = m.mul(&self.y, &s, &mut allocator);

        // RR = R^2
        let rr = m.mul(&r, &r, &mut allocator);

        // B = (X1+R)^2-XX-RR
        let b = {
            // t3 = X1+R
            let t3 = m.add(&self.x, &r, &mut allocator);
            // t4 = t3^2
            let t4 = m.mul(&t3, &t3, &mut allocator);
            // t5 = t4-XX
            let t5 = m.sub(&t4, &xx, &mut allocator);

            // B = t5-RR
            m.sub(&t5, &rr, &mut allocator)
        };

        // h = w^2-2*B
        let h = {
            // t6 = w^2
            let t6 = m.mul(&w, &w, &mut allocator);
            // t7 = 2*B
            let t7 = m.add(&b, &b, &mut allocator);

            // h = t6-t7
            m.sub(&t6, &t7, &mut allocator)
        };

        // X3 = h*s
        let x3 = m.mul(&h, &s, &mut allocator);

        // Y3 = w*(B-h)-2*RR
        let y3 = {
            // t8 = B-h
            let t8 = m.sub(&b, &h, &mut allocator);
            // t9 = 2*RR
            let t9 = m.add(&rr, &rr, &mut allocator);
            // t10 = w*t8
            let t10 = m.mul(&w, &t8, &mut allocator);
            // Y3 = t10-t9
            m.sub(&t10, &t9, &mut allocator)
        };

        // Z3 = sss
        let z3 = sss;

        Self {
            x: x3,
            y: y3,
            z: z3,
            inf: self.inf,
        }
    }

    // "add-2007-bl"
    fn add(&self, p2: &Self, curve_a: &SecureBigUint, m: &SecureMontgomeryModulo) -> Self {
        let mut allocator = HeapAllocator {};

        // U1 = X1*Z2
        let u1 = m.mul(&self.x, &p2.z, &mut allocator);

        // U2 = X2*Z1
        let u2 = m.mul(&p2.x, &self.z, &mut allocator);

        // S1 = Y1*Z2
        let s1 = m.mul(&self.y, &p2.z, &mut allocator);

        // S2 = Y2*Z1
        let s2 = m.mul(&p2.y, &self.z, &mut allocator);

        // ZZ = Z1*Z2
        let zz = m.mul(&self.z, &p2.z, &mut allocator);

        // T = U1+U2
        let t = m.add(&u1, &u2, &mut allocator);

        // TT = T^2
        let tt = m.mul(&t, &t, &mut allocator);

        // M = S1+S2
        let M = m.add(&s1, &s2, &mut allocator);

        // R = TT-U1*U2+a*ZZ^2
        let r = {
            // t0 = ZZ^2
            let t0 = m.mul(&zz, &zz, &mut allocator);

            // t1 = a*t0
            let t1 = m.mul(&curve_a, &t0, &mut allocator);

            // t2 = U1*U2
            let t2 = m.mul(&u1, &u2, &mut allocator);

            // t3 = TT-t2
            let t3 = m.sub(&tt, &t2, &mut allocator);

            // R = t3+t1
            m.add(&t3, &t1, &mut allocator)
        };

        // F = ZZ*M
        let f = m.mul(&zz, &M, &mut allocator);

        // L = M*F
        let l = m.mul(&M, &f, &mut allocator);

        // LL = L^2
        let ll = m.mul(&l, &l, &mut allocator);

        // G = (T+L)^2-TT-LL
        let g = {
            let tmp = m.add(&t, &l, &mut allocator);
            let tmp2 = m.mul(&tmp, &tmp, &mut allocator);
            let tmp3 = m.sub(&tmp2, &tt, &mut allocator);
            m.sub(&tmp3, &ll, &mut allocator)
        };

        // W = 2*R^2-G
        let w = {
            let tmp = m.mul(&r, &r, &mut allocator);
            let tmp2 = m.add(&tmp, &tmp, &mut allocator);
            m.sub(&tmp2, &g, &mut allocator)
        };

        // X3 = 2*F*W
        let x3 = {
            let tmp = m.mul(&f, &w, &mut allocator);
            m.add(&tmp, &tmp, &mut allocator)
        };

        // Y3 = R*(G-2*W)-2*LL
        let y3 = {
            let tmp = m.add(&w, &w, &mut allocator); // 2*W
            let tmp = m.sub(&g, &tmp, &mut allocator); // (G-2*W)
            let tmp = m.mul(&tmp, &r, &mut allocator); // R*(G-2*W)

            let tmp = m.sub(&tmp, &ll, &mut allocator);
            let tmp = m.sub(&tmp, &ll, &mut allocator);

            tmp
        };

        // Z3 = 4*F*F^2
        let z3 = {
            let mut tmp = SecureBigUint::from_usize(4, m.modulus().bit_width(), &mut allocator);
            m.to_montgomery_form(&mut tmp, &mut allocator);

            let tmp = m.mul(&tmp, &f, &mut allocator); // 4*F
            let tmp = m.mul(&tmp, &f, &mut allocator); //
            let tmp = m.mul(&tmp, &f, &mut allocator);

            tmp
        };

        let mut out = Self {
            x: x3,
            y: y3,
            z: z3,
            inf: false,
        };

        // TODO: Does this need to be conditional?
        self.copy_if(p2.inf, &mut out);
        p2.copy_if(self.inf, &mut out);

        out
    }
}

/// Format used for encoding/decoding ECDSA signatures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EllipticCurveSignatureFormat {
    /// The signature parameters are packed into a variable length DER encoded
    /// PKIX1Algorithms2008::ECDSA_Sig_Value struct.
    ///
    /// (used in X509 certificates / TLS)
    X509,

    /// The signature parameters 'r' and 's' are concatened into a fixed length
    /// string 'r | s' by interpreting each integer as big endian.
    ///
    /// (used in JOSE algorithms, DNSSEC)
    Concatenated,
}

/// Integers in this struct are assumed to be in the range [1, n).
#[derive(PartialEq, Debug, Clone)]
pub struct EllipticCurveSignature {
    pub r: SecureBigUint,
    pub s: SecureBigUint,
}

impl EllipticCurveSignature {
    pub fn encode(&self, format: EllipticCurveSignatureFormat) -> Vec<u8> {
        match format {
            EllipticCurveSignatureFormat::X509 => {
                let sig = pkix::PKIX1Algorithms2008::ECDSA_Sig_Value {
                    r: BigUint::from_le_bytes(&self.r.to_le_bytes()).into(),
                    s: BigUint::from_le_bytes(&self.s.to_le_bytes()).into(),
                };

                sig.to_der()
            }
            EllipticCurveSignatureFormat::Concatenated => {
                let mut out = vec![];
                out.extend_from_slice(&self.r.to_be_bytes());
                out.extend_from_slice(&self.s.to_be_bytes());
                out
            }
        }
    }

    pub fn decode(
        signature: &[u8],
        format: EllipticCurveSignatureFormat,
        group: &EllipticCurveGroup,
    ) -> Result<Self> {
        let (r, s) = match format {
            EllipticCurveSignatureFormat::X509 => {
                // TODO: We should allow passing in an Into<Bytes> to avoid cloning the
                // data here.
                let parsed =
                    pkix::PKIX1Algorithms2008::ECDSA_Sig_Value::from_der(signature.into())?;

                let mut r = SecureBigUint::from_le_bytes(
                    &parsed.r.to_uint()?.to_le_bytes(),
                    &mut HeapAllocator {},
                );
                let mut s = SecureBigUint::from_le_bytes(
                    &parsed.s.to_uint()?.to_le_bytes(),
                    &mut HeapAllocator {},
                );

                // NOTE: ASN.1 integers are stored using a minimum number of bytes.
                // This should not panic as we already verified that the numbers aren't larger
                // than the modulus.
                r.extend(group.n.bit_width(), &mut HeapAllocator {});
                s.extend(group.n.bit_width(), &mut HeapAllocator {});

                (r, s)
            }
            EllipticCurveSignatureFormat::Concatenated => {
                let int_size = ceil_div(group.n.bit_width(), 8);
                if signature.len() != 2 * int_size {
                    return Err(err_msg("Signature is the wrong length"));
                }

                let r =
                    SecureBigUint::from_be_bytes(&signature[0..int_size], &mut HeapAllocator {});
                let s = SecureBigUint::from_be_bytes(&signature[int_size..], &mut HeapAllocator {});
                (r, s)
            }
        };

        // Both must be in the range [1, n).
        let one = SecureBigUint::from_constant(1);
        if r < one || r >= group.n || s < one || s >= group.n {
            return Err(err_msg("Signature out of range"));
        }

        Ok(Self { r, s })
    }
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
        Ok(self.generate_private_key().await)
    }

    fn public_value(&self, secret: &[u8]) -> Result<Vec<u8>> {
        // TODO: Check that this is correct for usage in TLS.

        let sk = self.decode_scalar(secret)?;

        // Compute 'public_point = secret_scalar * base_point'.
        let p = self.scalar_mul_base_point(&sk, &mut HeapAllocator {});

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
        let v_x = self.scalar_mul_point(&s, &p, &mut HeapAllocator {}).x;

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
        let p = SecureBigUint::from_be_bytes(p_str, &mut HeapAllocator {});
        let a = SecureBigUint::from_be_bytes(a_str, &mut HeapAllocator {});
        let b = SecureBigUint::from_be_bytes(b_str, &mut HeapAllocator {});
        let g_x = SecureBigUint::from_be_bytes(g_x_str, &mut HeapAllocator {});
        let g_y = SecureBigUint::from_be_bytes(g_y_str, &mut HeapAllocator {});
        let n = SecureBigUint::from_be_bytes(n_str, &mut HeapAllocator {});

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

    /// Generates a new random private key.
    /// (scalar multiplier in the random [1, n-1]).
    ///
    /// The 'official' guidance on how to do this is in
    /// https://nvlpubs.nist.gov/nistpubs/SpecialPublications/nist.sp.800-56Ar3.pdf
    ///
    /// We implement this using the 5.6.1.2.1 section (extra random bits)
    /// method.
    pub async fn generate_private_key(&self) -> Vec<u8> {
        let l = ceil_div(self.n.bit_width() + 64, 8);

        let mut c_raw = vec![0u8; l];
        crate::random::global_rng().generate_bytes(&mut c_raw).await;

        let c = SecureBigUint::from_le_bytes(&c_raw, &mut HeapAllocator {});

        let one = SecureBigUint::from_constant(1);
        let n_minus_1 = self.n.sub(&one, &mut HeapAllocator {});

        let mut x = c.rem(&n_minus_1, &mut HeapAllocator {}) + &one;
        x.truncate(self.n.bit_width());

        // Must be the opposite encoding as decode_scalar()
        x.to_be_bytes()
    }

    /// See https://en.wikipedia.org/wiki/Elliptic_Curve_Digital_Signature_Algorithm.
    pub async fn create_signature(
        &self,
        private_key: &[u8],
        data: &[u8],
        signature_format: EllipticCurveSignatureFormat,
        hasher: &mut dyn Hasher,
    ) -> Result<Vec<u8>> {
        let mut arena = Arena::new(8192);

        let digest = hasher.finish_with(data);

        // TODO: Maybe switch to using a deterministic randomness as in https://www.rfc-editor.org/rfc/rfc6979.html

        for _ in 0..4 {
            let k = self.decode_scalar(&self.generate_private_key().await)?;
            if let Some(val) = self.create_signature_with(
                private_key,
                &digest,
                signature_format,
                &k,
                &mut arena.allocator(),
            )? {
                return Ok(val);
            }
        }

        Err(err_msg("Exhausted tried to make a signature"))
    }

    pub fn create_signature_with<'a, A: Allocator<'a>>(
        &self,
        private_key: &[u8],
        digest: &[u8],
        signature_format: EllipticCurveSignatureFormat,
        random: &SecureBigUint,
        allocator: &mut A,
    ) -> Result<Option<Vec<u8>>> {
        let mut allocator = allocator.sub_allocator();

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
            let mut v = SecureBigUint::from_be_bytes(digest, &mut HeapAllocator {});
            v.shr_n(8 * digest.len() - z_length);
            // v.truncate(self.n.bit_width());
            v.reduce_once(&self.n, &mut HeapAllocator {});
            v
        };

        /// x_1 = (random_scalar*base_point).x
        let x_1 = self.scalar_mul_base_point(random, &mut allocator).x;

        /// NOTE: x_1 was computed 'mod p' where 'p' may be much larger than
        /// 'n'. TODO: When n is only somewhat smaller, use barett
        /// reduction
        let r = SecureModulo::new(&self.n).rem(&x_1, &mut HeapAllocator {});

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

            modulo.to_montgomery_form(&mut random, &mut allocator);
            modulo.to_montgomery_form(&mut r, &mut allocator);
            modulo.to_montgomery_form(&mut z, &mut allocator);
            modulo.to_montgomery_form(&mut d_a, &mut allocator);

            // TODO: Given we are doing so few multiplications here, it is probably more
            // effiicent to use baret reduction.
            let s = modulo.mul(
                &modulo.inv_public_prime_mod(&random, &mut allocator),
                &modulo.add(
                    &z,
                    &modulo.mul(&r, &d_a, &mut HeapAllocator {}),
                    &mut HeapAllocator {},
                ),
                &mut HeapAllocator {},
            );

            modulo.from_montgomery_form(&s, &mut HeapAllocator {})
        };

        if s.is_zero() {
            return Ok(None);
        }

        let sig = EllipticCurveSignature { r, s }.encode(signature_format);
        Ok(Some(sig))
    }

    // ECDSA
    pub fn verify_signature(
        &self,
        public_key: &[u8],
        signature: &[u8],
        signature_format: EllipticCurveSignatureFormat,
        data: &[u8],
        hasher: &mut dyn Hasher,
    ) -> Result<bool> {
        hasher.update(data);
        let digest = hasher.finish();
        self.verify_digest_signature(public_key, signature, signature_format, &digest)
    }

    /// TODO: Consider offering a non-constant time version of there when it is
    /// not important to avoid leaking the message.
    pub fn verify_digest_signature(
        &self,
        public_key: &[u8],
        signature: &[u8],
        signature_format: EllipticCurveSignatureFormat,
        digest: &[u8],
    ) -> Result<bool> {
        // TODO: We should allow passing in an Into<Bytes> to avoid cloning the
        // data here.
        let (r, s) = {
            let sig = EllipticCurveSignature::decode(signature, signature_format, self)?;
            (sig.r, sig.s)
        };

        /// Length of 'z' in bits (same as 'n').
        let z_length = self.n.value_bits(); // NOTE: 'n' is publicly known.
        if z_length > 8 * digest.len() {
            return Err(err_msg("Message digest too short"));
        }

        // z_length leftmost bits of digest
        let z = {
            let mut v = SecureBigUint::from_be_bytes(digest, &mut HeapAllocator {});
            v.shr_n(8 * digest.len() - z_length);
            // v.truncate(self.n.bit_width());
            v.reduce_once(&self.n, &mut HeapAllocator {});
            v
        };

        // u_1 = z s^-1 mod n
        // u_2 = r s^-1 mod n
        let (u_1, u_2) = {
            let m = SecureMontgomeryModulo::new(&self.n);

            let mut r = r.clone();
            let mut s = s.clone();
            let mut z = z.clone();
            m.to_montgomery_form(&mut r, &mut HeapAllocator {});
            m.to_montgomery_form(&mut s, &mut HeapAllocator {});
            m.to_montgomery_form(&mut z, &mut HeapAllocator {});

            let s_inv = m.inv_public_prime_mod(&s, &mut HeapAllocator {});

            (
                m.from_montgomery_form(
                    &m.mul(&z, &s_inv, &mut HeapAllocator {}),
                    &mut HeapAllocator {},
                ),
                m.from_montgomery_form(
                    &m.mul(&r, &s_inv, &mut HeapAllocator {}),
                    &mut HeapAllocator {},
                ),
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

            m.to_montgomery_form(&mut g.x, &mut HeapAllocator {});
            m.to_montgomery_form(&mut g.y, &mut HeapAllocator {});
            m.to_montgomery_form(&mut point.x, &mut HeapAllocator {});
            m.to_montgomery_form(&mut point.y, &mut HeapAllocator {});

            // TODO: Perform all three of these operations in projective space and then only
            // convert back from projective form once at the end.
            let a = self.scalar_mul_point_impl(&u_1, &g, &m, &mut HeapAllocator {});
            let b = self.scalar_mul_point_impl(&u_2, &point, &m, &mut HeapAllocator {});
            let c = self.add_points(&a, &b, &m, &mut HeapAllocator {});

            EllipticCurvePoint {
                x: m.from_montgomery_form(&c.x, &mut HeapAllocator {}),
                y: m.from_montgomery_form(&c.y, &mut HeapAllocator {}),
                inf: c.inf,
            }
        };

        // TODO: Verify not at infinity.

        // Valid if 'r mod n === output_point.x mod n'
        // TODO: Use modular equivalence
        let mut modulo = SecureModulo::new(&self.n);
        Ok(modulo.rem(&r, &mut HeapAllocator {})
            == modulo.rem(&output_point.x, &mut HeapAllocator {}))
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

        let v = SecureBigUint::from_be_bytes(data, &mut HeapAllocator {});
        if v >= self.n {
            return Err(err_msg("Scalar larger than group order"));
        }

        Ok(v)
    }

    // TODO: Currently only needed for the JWK implementation. Consider making
    // private again.
    pub fn decode_point(&self, data: &[u8]) -> Result<EllipticCurvePoint> {
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

            let x = SecureBigUint::from_be_bytes(&data[1..(nbytes + 1)], &mut HeapAllocator {});
            let y = SecureBigUint::from_be_bytes(&data[(nbytes + 1)..], &mut HeapAllocator {});

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
    fn add_points<'a, A: Allocator<'a>>(
        &self,
        p: &EllipticCurvePoint,
        q: &EllipticCurvePoint,
        m: &SecureMontgomeryModulo,
        allocator: &mut A,
    ) -> EllipticCurvePoint {
        let mut allocator = allocator.sub_allocator();

        // slope = (y_q - y_p) / (x_q - x_p)
        let slope = m.mul(
            &m.sub(&q.y, &p.y, &mut allocator),
            &m.inv_public_prime_mod(&m.sub(&q.x, &p.x, &mut allocator), &mut allocator),
            &mut allocator,
        );
        Self::intersecting_point(p, q, &slope, m)
    }

    /// NOTE: 'p' should already be in Montgomery form.
    fn double_point<'a, A: Allocator<'a>>(
        &self,
        p: &EllipticCurvePoint,
        m: &SecureMontgomeryModulo,
        curve_a: &SecureBigUint,
        allocator: &mut A,
    ) -> EllipticCurvePoint {
        let mut allocator = allocator.sub_allocator();

        // TODO: Instead just use addition rather than multiplying by these small
        // constants.
        let mut two = SecureBigUint::from_usize(2, self.p.bit_width(), &mut allocator);
        let mut three = SecureBigUint::from_usize(3, self.p.bit_width(), &mut allocator);
        m.to_montgomery_form(&mut two, &mut allocator);
        m.to_montgomery_form(&mut three, &mut allocator);

        // slope = (3 x_P^2 + a) / (2 y_P)
        let slope = m.mul(
            &m.add(
                &m.mul(&three, &m.mul(&p.x, &p.x, &mut allocator), &mut allocator),
                &curve_a,
                &mut allocator,
            ),
            &m.inv_public_prime_mod(&m.mul(&two, &p.y, &mut allocator), &mut allocator),
            &mut allocator,
        );

        Self::intersecting_point(p, p, &slope, m)
    }

    /// Internal shared logic of the above two methods.
    fn intersecting_point<S: StorageType>(
        p: &EllipticCurvePoint,
        q: &EllipticCurvePoint,
        slope: &SecureBigUint<S>,
        m: &SecureMontgomeryModulo,
    ) -> EllipticCurvePoint {
        // x_R = slope^2 - (x_P + x_Q)
        let x = m.sub(
            &m.mul(slope, slope, &mut HeapAllocator {}),
            &m.add(&p.x, &q.x, &mut HeapAllocator {}),
            &mut HeapAllocator {},
        );

        // y_R = slope*(x_P - x_R) - y_P
        let y = m.sub(
            &m.mul(
                slope,
                &m.sub(&p.x, &x, &mut HeapAllocator {}),
                &mut HeapAllocator {},
            ),
            &p.y,
            &mut HeapAllocator {},
        );

        let mut out = EllipticCurvePoint { x, y, inf: false };

        p.copy_if(q.is_inf(), &mut out);
        q.copy_if(p.is_inf(), &mut out);

        out
    }

    /// Multiplies an arbitrary point with a scalar.
    ///
    /// Internally uses the montgomery ladder approach.
    ///
    /// - 'd': Input scalar in normal form (NOT MONTGOMERY FORM)
    /// - 'p': Curve point in montgomery form.
    ///
    /// Returns the multiplied point in montgomery form.
    fn scalar_mul_point_impl<'a, A: Allocator<'a>>(
        &self,
        d: &SecureBigUint,
        p: &EllipticCurvePoint,
        m: &SecureMontgomeryModulo,
        allocator: &mut A,
    ) -> EllipticCurvePoint {
        let mut r_0 = EllipticCurvePoint::inf(self.p.bit_width());

        // NOTE: 'p' passed in as montgomery form.
        let mut r_1 = p.clone();

        // Only if using projective coordinates.
        let mut r_0 = EllipticCurveProjectivePoint::from_affine(r_0, &m);
        let mut r_1 = EllipticCurveProjectivePoint::from_affine(r_1, &m);

        let mut curve_a = self.curve.a.clone();
        m.to_montgomery_form(&mut curve_a, allocator);

        let mut swap = false;

        for i in (0..d.bit_width()).rev() {
            let d_i = d.bit(i) != 0;
            swap ^= d_i;

            r_0.swap_if(swap, &mut r_1);
            swap = d_i;

            // Only if using projective coordinates.
            r_1 = r_0.add(&r_1, &curve_a, m);
            r_0 = r_0.double(&curve_a, m);
            // For affine coordinate calculations.
            // r_1 = self.add_points(&r_0, &r_1, m, allocator);
            // r_0 = self.double_point(&r_0, m, &curve_a, allocator);
        }

        r_0.swap_if(swap, &mut r_1);

        // Only if using projective coordinates.
        let r_0 = r_0.to_affine(m);

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
        let lhs = m.mul(&p.y, &p.y, &mut HeapAllocator {});

        // x^3 + a*x + b
        let rhs = {
            m.add(
                &m.pow(
                    &p.x,
                    &SecureBigUint::from_constant(3),
                    &mut HeapAllocator {},
                ),
                &m.add(
                    &m.mul(&self.curve.a, &p.x, &mut HeapAllocator {}),
                    &self.curve.b,
                    &mut HeapAllocator {},
                ),
                &mut HeapAllocator {},
            )
        };

        // NOTE: Both are reduced by the modulus.
        lhs == rhs
    }

    /// Multiples the given point 'p' by itself 'd' times.
    pub fn scalar_mul_point<'a, A: Allocator<'a>>(
        &self,
        d: &SecureBigUint,
        p: &EllipticCurvePoint,
        allocator: &mut A,
    ) -> EllipticCurvePoint {
        let modulo = SecureMontgomeryModulo::new(&self.p);

        let mut p = p.clone();
        modulo.to_montgomery_form(&mut p.x, allocator);
        modulo.to_montgomery_form(&mut p.y, allocator);

        let p = self.scalar_mul_point_impl(d, &p, &modulo, allocator);

        EllipticCurvePoint {
            x: modulo.from_montgomery_form(&p.x, &mut HeapAllocator {}),
            y: modulo.from_montgomery_form(&p.y, &mut HeapAllocator {}),
            inf: p.inf,
        }
    }

    /// Multiplies the base curve point by itself 'd' times.
    pub fn scalar_mul_base_point<'a, A: Allocator<'a>>(
        &self,
        d: &SecureBigUint,
        allocator: &mut A,
    ) -> EllipticCurvePoint {
        let mut g = self.g.clone();

        let modulo = SecureMontgomeryModulo::new(&self.p);
        // TODO: Precompute this.
        modulo.to_montgomery_form(&mut g.x, allocator);
        modulo.to_montgomery_form(&mut g.y, allocator);

        let p = self.scalar_mul_point_impl(d, &g, &modulo, allocator);

        EllipticCurvePoint {
            x: modulo.from_montgomery_form(&p.x, &mut HeapAllocator {}),
            y: modulo.from_montgomery_form(&p.y, &mut HeapAllocator {}),
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
            SecureBigUint::from_usize(v, 32, &mut HeapAllocator {})
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

        let out = ecc.scalar_mul_base_point(&k, &mut HeapAllocator {});
        assert_eq!(out.x, x);
        assert_eq!(out.y, y);
    }

    #[test]
    fn secp256r1_test() {
        let k = SecureBigUint::from_str(
            "29852220098221261079183923314599206100666902414330245206392788703677545185283",
            256,
            &mut HeapAllocator {},
        )
        .unwrap();
        let x = SecureBigUint::from_be_bytes(
            &hex!("9EACE8F4B071E677C5350B02F2BB2B384AAE89D58AA72CA97A170572E0FB222F"),
            &mut HeapAllocator {},
        );
        let y = SecureBigUint::from_be_bytes(
            &hex!("1BBDAEC2430B09B93F7CB08678636CE12EAAFD58390699B5FD2F6E1188FC2A78"),
            &mut HeapAllocator {},
        );

        let ecc = EllipticCurveGroup::secp256r1();

        let out = ecc.scalar_mul_base_point(&k, &mut HeapAllocator {});

        assert_eq!(out.x, x);
        assert_eq!(out.y, y);
    }

    #[testcase]
    async fn encoding_point_sizes() -> Result<()> {
        // In RFC 8446 Section 4.2.8.2, the size of the points is well defined.

        let mut test_cases = vec![
            (EllipticCurveGroup::secp256r1(), 32, 1 + 2 * 32),
            (EllipticCurveGroup::secp384r1(), 48, 1 + 2 * 48),
            (EllipticCurveGroup::secp521r1(), 66, 1 + 2 * 66),
        ];

        for (curve, expected_secret_size, expected_size) in test_cases {
            let secret = curve.secret_value().await?;
            assert_eq!(secret.len(), expected_secret_size);
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
                EllipticCurveSignatureFormat::X509,
                &SecureBigUint::from_be_bytes(&random, &mut HeapAllocator {}),
                &mut HeapAllocator {},
            )?
            .unwrap();

        assert_eq!(new_sig, sig);

        let point = hex!("04C5C9B38D3603FCCD6994CBB9594E152B658721E483669BB42728520F484B537647EC816E58A8284D3B89DFEDB173AFDC214ECA95A836FA7C");

        // let mut hasher = crate::sha256::SHA256Hasher::default();

        assert!(curve
            .verify_digest_signature(&point, &sig, EllipticCurveSignatureFormat::X509, &digest)
            .unwrap());

        Ok(())
    }

    #[testcase]
    async fn ecdsa_sign_test() -> Result<()> {
        let group = EllipticCurveGroup::secp256r1();

        let secret = group.secret_value().await?;
        let data = b"hello world";

        let mut hasher = crate::sha256::SHA256Hasher::default();

        let signature = group
            .create_signature(
                &secret,
                data,
                EllipticCurveSignatureFormat::X509,
                &mut hasher,
            )
            .await?;

        let mut hasher2 = crate::sha256::SHA256Hasher::default();

        let public = group.public_value(&secret)?;

        assert!(group.verify_signature(
            &public,
            &signature,
            EllipticCurveSignatureFormat::X509,
            data,
            &mut hasher2
        )?);

        Ok(())
    }
}
