use math::big::*;
use crate::random::*;
use crate::dh::*;
use common::errors::*;
use common::ceil_div;
use std::marker::PhantomData;
use common::LeftPad;

/// Parameters of an elliptic curve of the form:
/// y^2 = x^3 + a*x + b
pub struct EllipticCurve {
	pub a: BigUint,
	pub b: BigUint
}

impl EllipticCurve {
	/// See https://en.wikipedia.org/wiki/Elliptic_curve_point_multiplication#Point_addition
	/// This includes support for doubling points.
	fn add_points(&self, p: &EllipticCurvePoint,
				  q: &EllipticCurvePoint, m: &Modulo) -> EllipticCurvePoint {
		if p.is_inf() {
			return q.clone();
		}
		if q.is_inf() {
			return p.clone();
		}

		let s = if p == q {
			m.div(&m.add_into(m.mul(&3.into(), &m.pow(&p.x, &2.into())),
							  &self.a),
				  &m.mul(&2.into(), &p.y))
		} else {
			m.div(&m.sub(&q.y, &p.y), &m.sub(&q.x, &p.x))
		};

		let x = m.sub_into(m.pow(&s, &2.into()),
						   &m.add(&p.x, &q.x));
		let y = m.sub_into(m.mul(&s, &m.sub(&p.x, &x)),
						   &p.y);

		EllipticCurvePoint { x, y }
	}

	/// Scalar*point multiplication using the 'double-and-add' method
	/// TODO: Switch to using the montgomery ladder method.
	pub fn scalar_mul(&self, d: &BigUint, P: &EllipticCurvePoint,
					  m: &Modulo) -> EllipticCurvePoint {
		let mut q = EllipticCurvePoint::zero();
		for i in (0..d.nbits()).rev() {
			q = self.add_points(&q, &q, m);
			if d.bit(i) == 1 {
				q = self.add_points(&q, P, m);
			}
		}

		q
	}
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct EllipticCurvePoint {
	pub x: BigUint,
	pub y: BigUint
}

impl EllipticCurvePoint {
	// TODO: This could be used by a timing attack to reveal byte by byte if 
	// parts of the point are zero.
	pub fn is_inf(&self) -> bool {
		self.x.is_zero() && self.y.is_zero()
	}
	pub fn zero() -> Self {
		EllipticCurvePoint { x: BigUint::zero(), y: BigUint::zero() }
	}
}

/// Parameters for a group of points on an elliptic curve definited over a
/// finite field of integers.
pub struct EllipticCurveGroup {
	/// Base curve.
	curve: EllipticCurve,
	/// Prime number which is the size of the finite field (all operations are performed 'mod p').
	p: BigUint,
	/// Base point
	g: EllipticCurvePoint,
	// Order
	n: BigUint,
	// Cofactor
	k: usize
}

// For P-256, this means that each of X and Y use
// 32 octets, padded on the left by zeros if necessary.  For P-384, they
// take 48 octets each.  For P-521, they take 66 octets each.

#[async_trait]
impl DiffieHellmanFn for EllipticCurveGroup {

	async fn secret_value(&self) -> Result<Vec<u8>> {
		assert!(self.k == 1);
		let two = BigUint::from(2);
		let n = secure_random_range(&two, &self.n).await?;
		Ok(n.to_be_bytes().left_pad(self.p.min_bytes(), 0))
	}

	fn public_value(&self, secret: &[u8]) -> Result<Vec<u8>> {
		unimplemented!("");
	}

	// TODO: Validate tht point is non-infinity
	// TODO: For TLS, shared secret should be the X coordinate of this only!!
	fn shared_secret(&self, secret: &[u8], public: &[u8]) -> Result<Vec<u8>> {
		unimplemented!("");
	}
}

use asn::encoding::DERReadable;

impl EllipticCurveGroup {

	// ECDSA
	pub fn verify_signature(&self, public_key: &[u8], signature: &[u8],
							data: &[u8], hasher: &mut dyn crate::hasher::Hasher)
		-> Result<bool> {

		// TODO: We should allow passing in an Into<Bytes> to avoid cloning the
		// data here.
		let (r, s) = {
			let parsed = pkix::PKIX1Algorithms2008::ECDSA_Sig_Value::from_der(
				signature.into())?;
			(parsed.r.to_uint()?, parsed.s.to_uint()?)
		};

		hasher.update(data);
		let mut digest = hasher.finish();

		let L_z = self.n.nbits();
		assert_eq!(L_z % 8, 0);
		assert!(L_z <= 8*digest.len());
		digest.truncate(L_z / 8);

		let z = BigUint::from_be_bytes(&digest);
		let modulo = Modulo::new(&self.n);
		let u_1 = modulo.div(&z, &s);
		let u_2 = modulo.div(&r, &s);

		// TODO: Validate that n x point = identity?
		let point = self.decode_point(public_key)?;
		let output_point = self.curve.add_points(
			&self.curve.scalar_mul(&u_1, &self.g, &Modulo::new(&self.p)),
			&self.curve.scalar_mul(&u_2, &point, &Modulo::new(&self.p)), &Modulo::new(&self.p));

		// TODO: Use modular equivalence
		Ok(modulo.rem(&r) == modulo.rem(&output_point.x))
	}

	fn decode_scalar(&self, data: &[u8]) -> Result<BigUint> {
		if data.len() != self.p.min_bytes() {
			return Err(err_msg("Scalar wrong size"));
		}

		Ok(BigUint::from_be_bytes(data))
	}

	// TODO: Not used anywhere right now.
	fn decode_point(&self, data: &[u8]) -> Result<EllipticCurvePoint> {
		if data.len() <= 1 {
			return Err(err_msg("Point too small"));
		}

		let nbytes = ceil_div(self.p.nbits(), 8);
		let x1 = if data[0] == 4 {
			// Uncompressed form
			// TODO: For TLS 1.3, this is the only supported format
			if data.len() != 1 + 2 * nbytes {
				return Err(err_msg("Point data too small"));
			}

			// TODO:
			let x = BigUint::from_be_bytes(&data[1..(nbytes + 1)]);
			let y = BigUint::from_be_bytes(&data[(nbytes + 1)..]);

			EllipticCurvePoint { x, y }
		} else if data[0] == 2 || data[0] == 3 {
			// Compressed form.
			// Contains only X, data[0] contains the LSB of Y.
			if data.len() != 1 + nbytes {
				return Err(err_msg("Point data too small"));
			}

			// TODO: Off by one
			let x = BigUint::from_be_bytes(&data[1..nbytes]);

			// Compute y^2 from the x.
			let y2 = (&x).pow(&3.into()) + &(&self.curve.a*&x)
				+ &self.curve.b;

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
		} else {
			return Err(format_err!("Unknown point format {}", data[0]));
		};

		let p = x1;

		if !self.verify_point(&p) {
			return Err(err_msg("Invalid point"));
		}

		Ok(p)
	}

	// TODO: Output uncompressed, but with padding
	// fn encode_point(&self )


	fn from_hex(p_str: &str, a_str: &str, b_str: &str, g_x_str: &str,
				g_y_str: &str, n_str: &str, h: usize) -> Self {
		let p = BigUint::from_be_bytes(&hex::decode(p_str).unwrap());
		let a = BigUint::from_be_bytes(&hex::decode(a_str).unwrap());
		let b = BigUint::from_be_bytes(&hex::decode(b_str).unwrap());
		let g_x = BigUint::from_be_bytes(&hex::decode(g_x_str).unwrap());
		let g_y = BigUint::from_be_bytes(&hex::decode(g_y_str).unwrap());
		let n = BigUint::from_be_bytes(&hex::decode(n_str).unwrap());

		EllipticCurveGroup {
			curve: EllipticCurve { a, b },
			p,
			g: EllipticCurvePoint { x: g_x, y: g_y },
			n,
			k: h
		}
	}

	pub fn secp192r1() -> Self {
		Self::from_hex(
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFFFFFFFFFF",
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFFFFFFFFFC",
			"64210519E59C80E70FA7E9AB72243049FEB8DEECC146B9B1",
			"188DA80EB03090F67CBF20EB43A18800F4FF0AFD82FF1012",
			"07192B95FFC8DA78631011ED6B24CDD573F977A11E794811",
			"FFFFFFFFFFFFFFFFFFFFFFFF99DEF836146BC9B1B4D22831",
			1)
	}

	pub fn secp224r1() -> Self {
		Self::from_hex(
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF000000000000000000000001",
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFFFFFFFFFFFFFFFFFE",
			"B4050A850C04B3ABF54132565044B0B7D7BFD8BA270B39432355FFB4",
			"B70E0CBD6BB4BF7F321390B94A03C1D356C21122343280D6115C1D21",
			"BD376388B5F723FB4C22DFE6CD4375A05A07476444D5819985007E34",
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFF16A2E0B8F03E13DD29455C5C2A3D",
			1)
	}

	pub fn secp256r1() -> Self {
		Self::from_hex(
			"FFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFF",
			"FFFFFFFF00000001000000000000000000000000FFFFFFFFFFFFFFFFFFFFFFFC",
			"5AC635D8AA3A93E7B3EBBD55769886BC651D06B0CC53B0F63BCE3C3E27D2604B",
			"6B17D1F2E12C4247F8BCE6E563A440F277037D812DEB33A0F4A13945D898C296",
			"4FE342E2FE1A7F9B8EE7EB4A7C0F9E162BCE33576B315ECECBB6406837BF51F5",
			"FFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551",
			1)
	}

	pub fn secp384r1() -> Self {
		Self::from_hex(
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFF\
			 FFFFFF0000000000000000FFFFFFFF",
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFF\
			 FFFFFF0000000000000000FFFFFFFC",
			"B3312FA7E23EE7E4988E056BE3F82D19181D9C6EFE8141120314088F5013875AC6\
			 56398D8A2ED19D2A85C8EDD3EC2AEF",
			"AA87CA22BE8B05378EB1C71EF320AD746E1D3B628BA79B9859F741E082542A3855\
			 02F25DBF55296C3A545E3872760AB7",
			"3617DE4A96262C6F5D9E98BF9292DC29F8F41DBD289A147CE9DA3113B5F0B8C00A\
			 60B1CE1D7E819D7A431D7C90EA0E5F",
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC7634D81F4372DDF58\
			 1A0DB248B0A77AECEC196ACCC52973",
			1)
	}

	pub fn secp521r1() -> Self {
		Self::from_hex(
			"01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\
			 FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\
			 ",
			"01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\
			 FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC\
			 ",
			"0051953EB9618E1C9A1F929A21A0B68540EEA2DA725B99B315F3B8B489918EF109\
			 E156193951EC7E937B1652C0BD3BB1BF073573DF883D2C34F1EF451FD46B503F00\
			 ",
			"00C6858E06B70404E9CD9E3ECB662395B4429C648139053FB521F828AF606B4D3D\
			 BAA14B5E77EFE75928FE1DC127A2FFA8DE3348B3C1856A429BF97E7E31C2E5BD66\
			 ",
			"011839296A789A3BC0045C8A5FB42C7D1BD998F54449579B446817AFBD17273E66\
			 2C97EE72995EF42640C550B9013FAD0761353C7086A272C24088BE94769FD16650\
			 ",
			"01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF\
			 FA51868783BF2F966B7FCC0148F709A5D03BB5C9B8899C47AEBB6FB71E91386409\
			 ",
			1)
	}



	/// Returns whether or not the given point is on the curve.
	/// TODO: There is also a 'point on curve' verification algorith here:
	/// https://en.wikipedia.org/wiki/Elliptic_Curve_Digital_Signature_Algorithm#Signature_verification_algorithm
	pub fn verify_point(&self, p: &EllipticCurvePoint) -> bool {
		// TODO: Must deal with three criteria:
		// 1. Not at infinity
		// 2. Both integers on the correct interval
		// 3. On the curve

		// Must not be at infinity
		if p.is_inf() {
			return false;
		}

		// Must be within the 'mod p' field.
		if p.x >= self.p || p.y >= self.p {
			return false;
		}

		let lhs = &p.y*&p.y;
		let rhs = (&p.x).pow(&3.into()) + &(&self.curve.a*&p.x) + &self.curve.b;
		(lhs % &self.p) == (rhs % &self.p)
	}

	pub fn curve_mul(&self, d: &BigUint) -> EllipticCurvePoint {
		self.curve.scalar_mul(d, &self.g, &Modulo::new(&self.p))
	}
}

struct EllipticCurveMultiplier {

}

// TODO: Don't forget to mask the last bit when decoding k or u?

// TODO: Now all of these are using modular arithmetic right now

// TODO: See https://en.wikipedia.org/wiki/Exponentiation_by_squaring#Montgomery's_ladder_technique as a method of doing power operations in constant time

// TODO: See start of https://tools.ietf.org/html/rfc7748#section-5 for the encode/decode functions


// pub fn decode_u_cord()

// TODO: Need a custom function for sqr (aka n^2)

// 32 bytes for X25519 and 56 bytes for X448


pub struct MontgomeryCurveGroup<C: MontgomeryCurveCodec> {
	/// Prime
	p: BigUint,
	/// U coordinate of the base point.
	u: BigUint,
	bits: usize,
	a24: BigUint,
	codec: PhantomData<C>
}

impl<C: MontgomeryCurveCodec> MontgomeryCurveGroup<C> {
	fn new(p: BigUint, u: BigUint, bits: usize, a24: BigUint) -> Self {
		Self { p, u, bits, a24, codec: PhantomData }
	}

	fn mul(&self, k: &BigUint, u: &BigUint) -> BigUint {
		curve_mul(k, u, &self.p, self.bits, &self.a24)
	}
}

impl MontgomeryCurveGroup<X25519> {
	pub fn x25519() -> Self {
		let p = BigUint::from(255).exp2() - &19.into();
		let u = BigUint::from(9);
		let bits = 255;
		let a24 = BigUint::from(121665);
		Self::new(p, u, bits, a24)
	}
}

impl MontgomeryCurveGroup<X448> {
	pub fn x448() -> Self {
		let p = BigUint::from(448).exp2()
			- BigUint::from(224).exp2() - BigUint::from(1);
		let u = BigUint::from(5);
		let bits = 448;
		let a24 = BigUint::from(39081);
		Self::new(p, u, bits, a24)
	}
}


#[async_trait]
impl<C: MontgomeryCurveCodec + Send + Sync>
DiffieHellmanFn for MontgomeryCurveGroup<C> {
	/// Creates a new 32byte private key
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

	fn shared_secret(&self, public: &[u8], secret: &[u8]) -> Result<Vec<u8>> {
		let u = C::decode_u_cord(public);
		let k = C::decode_scalar(secret);
		let out = self.mul(&k, &u);
		Ok(C::encode_u_cord(&out))

		// TODO: Validate shared secret is not all zero
		// ^ See https://tools.ietf.org/html/rfc7748#section-6.1 for how to do it securely
	}
}

pub trait MontgomeryCurveCodec {
	// NOTE: There is generally no need for encoding the scalar.
	fn decode_scalar(data: &[u8]) -> BigUint;

	fn encode_u_cord(u: &BigUint) -> Vec<u8>;
	fn decode_u_cord(data: &[u8]) -> BigUint;
}

pub struct X25519 {}

impl MontgomeryCurveCodec for X25519 {
	fn decode_scalar(data: &[u8]) -> BigUint {
		assert_eq!(data.len(), 32);

		let mut sdata = data.to_vec();
		sdata[0] &= 248;
		sdata[31] &= 127;
		sdata[31] |= 64;

		BigUint::from_le_bytes(&sdata)
	}

	// TODO: Must assert that it is 32 bytes and error out if it isn't.
	fn decode_u_cord(data: &[u8]) -> BigUint {
		assert_eq!(data.len(), 32);

		let mut sdata = data.to_vec();
		// Mask MSB in last byte (only applicable to X25519).
		sdata[31] &= 0x7f;

		BigUint::from_le_bytes(&sdata)
	}

	fn encode_u_cord(u: &BigUint) -> Vec<u8> {
		let mut data = u.to_le_bytes();
		assert!(data.len() <= 32);
		data.resize(32, 0);
		data
	}
}

pub struct X448 {}

impl MontgomeryCurveCodec for X448 {
	fn decode_scalar(data: &[u8]) -> BigUint {
		assert_eq!(data.len(), 56);

		let mut sdata = data.to_vec();
		sdata[0] &= 252;
		sdata[55] |= 128;

		BigUint::from_le_bytes(&sdata)
	}

	fn decode_u_cord(data: &[u8]) -> BigUint {
		assert_eq!(data.len(), 56);
		BigUint::from_le_bytes(data)
	}

	fn encode_u_cord(u: &BigUint) -> Vec<u8> {
		let mut data = u.to_le_bytes();
		assert!(data.len() <= 56);
		data.resize(56, 0);
		data
	}
}


fn curve_mul(k: &BigUint, u: &BigUint, p: &BigUint,
			 bits: usize, a24: &BigUint) -> BigUint {
	let modulo = Modulo::new(p);

	let x_1 = u;
	let mut x_2 = BigUint::from(1);
	let mut z_2 = BigUint::zero();
	let mut x_3 = u.clone();
	let mut z_3 = BigUint::from(1);
	let mut swap = BigUint::zero();

	let two = BigUint::from(2);
	
	for t in (0..bits).rev() {
		let k_t = BigUint::from(k.bit(t));
		swap ^= &k_t;

		tup!((x_2, x_3) = cswap(&modulo, &swap, x_2, x_3));
		tup!((z_2, z_3) = cswap(&modulo, &swap, z_2, z_3));
		swap = k_t;

		let A = modulo.add(&x_2, &z_2);
		let AA = modulo.pow(&A, &two);
		let B = modulo.sub(&x_2, &z_2);

		let BB = B.pow(&2.into());
		let E = modulo.sub(&AA, &BB);
		let C = modulo.add(&x_3, &z_3);
		let D = modulo.sub(&x_3, &z_3);
		let DA = modulo.mul(&D, &A);
		let CB = modulo.mul(&C, &B);
		x_3 = (&DA + &CB).pow(&two);
		// TODO: Here we can do a subtraction without cloning by taking ownership
		z_3 = modulo.mul(&x_1, &modulo.sub(&DA, &CB).pow(&two));
		x_2 = modulo.mul(&AA, &BB);
		z_2 = modulo.mul(&E, &(AA + &(a24 * &E)));
	}

	tup!((x_2, x_3) = cswap(&modulo, &swap, x_2, x_3));
	tup!((z_2, z_3) = cswap(&modulo, &swap, z_2, z_3));
	modulo.mul(&x_2, &modulo.pow(&z_2, &(p - &two)))
}

fn cswap(modulo: &Modulo, swap: &BigUint,
		 x_2: BigUint, x_3: BigUint)
-> (BigUint, BigUint) {

	if swap.is_one() {
		(x_3, x_2)
	} else {
		(x_2, x_3)
	}

	// TODO: Fix this
	// let dummy = mask(modulo, swap) & (x_2 ^ x_3);
	// (modulo.rem(&(x_2 ^ &dummy)), modulo.rem(&(x_3 ^ &dummy)))
}

fn mask(modulo: &Modulo, v: &BigUint) -> BigUint {
	modulo.sub(&BigUint::zero(), v)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::str::FromStr;
	use asn::encoding::DERWriteable;

	#[test]
	fn small_elliptic_curve_test() {
		let k = BigUint::from(2);
		let x = BigUint::from(80);
		let y = BigUint::from(10);

		let ecc = EllipticCurveGroup {
			curve: EllipticCurve { a: BigUint::from(2), b: BigUint::from(3) },
			p: BigUint::from(97),
			g: EllipticCurvePoint {
				x: BigUint::from(3),
				y: BigUint::from(6)
			},
			n: BigUint::from(100),
			k: 1
		};

		let out = ecc.curve_mul(&k);
		assert_eq!(out.x, x);
		assert_eq!(out.y, y);
	}

	#[test]
	fn secp256r1_test() {
		let k = BigUint::from_str(
			"29852220098221261079183923314599206100666902414330245206392788703677545185283").unwrap();
		let x = BigUint::from_be_bytes(&hex::decode(
			"9EACE8F4B071E677C5350B02F2BB2B384AAE89D58AA72CA97A170572E0FB222F").unwrap());
		let y = BigUint::from_be_bytes(&hex::decode(
			"1BBDAEC2430B09B93F7CB08678636CE12EAAFD58390699B5FD2F6E1188FC2A78").unwrap());

		let ecc = EllipticCurveGroup::secp256r1();
		let out = ecc.curve_mul(&k);

		assert_eq!(out.x, x);
		assert_eq!(out.y, y);
	}

	#[test]
	fn x25519_test() {
		assert_eq!(BigUint::from_le_bytes(
			&hex::decode("01").unwrap()).to_string(), "1");
		assert_eq!(BigUint::from_le_bytes(
			&hex::decode("0100000002").unwrap()).to_string(), "8589934593");

		let scalar = BigUint::from_str("31029842492115040904895560451863089656472772604678260265531221036453811406496").unwrap();
		let u_in = BigUint::from_str("34426434033919594451155107781188821651316167215306631574996226621102155684838").unwrap();

		let u_out = MontgomeryCurveGroup::x25519().mul(&scalar, &u_in);
		assert_eq!(hex::encode(u_out.to_le_bytes()), "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");

		let scalar2 = X25519::decode_scalar(&hex::decode("4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d").unwrap());
		assert_eq!(scalar2.to_string(), "35156891815674817266734212754503633747128614016119564763269015315466259359304");

		let u_in2 = X25519::decode_u_cord(&hex::decode("e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493").unwrap());
		assert_eq!(u_in2.to_string(), "8883857351183929894090759386610649319417338800022198945255395922347792736741");

		let u_out2 = MontgomeryCurveGroup::x25519().mul(&scalar2, &u_in2);
		assert_eq!(&X25519::encode_u_cord(&u_out2),
				   &hex::decode("95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957").unwrap());
	}

	#[test]
	fn ecdh_x25519_codec_test() {
		assert_eq!(
			X25519::decode_scalar(&hex::decode("a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4").unwrap()).to_string(),
			"31029842492115040904895560451863089656472772604678260265531221036453811406496");

		assert_eq!(
			X25519::decode_u_cord(&hex::decode("e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c").unwrap()).to_string(),
			"34426434033919594451155107781188821651316167215306631574996226621102155684838");
	}


	#[test]
	fn ecdh_x25519_test() {
		let alice_private = hex::decode(
			"77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a").unwrap();
		let alice_public = hex::decode(
			"8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a").unwrap();
		let bob_private = hex::decode(
			"5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb").unwrap();
		let bob_public = hex::decode(
			"de9edb7d7b7dc1b4d35b61c2ece435373f8343c85b78674dadfc7e146f882b4f").unwrap();
		let shared_secret = hex::decode(
			"4a5d9d5ba4ce2de1728e3bf480350f25e07e21c947d19e3376f09b3c1e161742").unwrap();

		let group = MontgomeryCurveGroup::x25519();
		assert_eq!(&group.public_value(&alice_private).unwrap(), &alice_public);
		assert_eq!(&group.public_value(&bob_private).unwrap(), &bob_public);
		assert_eq!(&group.shared_secret(&alice_public, &bob_private).unwrap(),
				   &shared_secret);
		assert_eq!(&group.shared_secret(&bob_public, &alice_private).unwrap(),
				   &shared_secret);
	}

	#[test]
	fn ecdsa_test() {
		// Test vectors grabbed from:
		// https://github.com/bcgit/bc-java/blob/master/core/src/test/java/org/bouncycastle/crypto/test/ECTest.java#L384
		// testECDSASecP224k1sha256

		let curve = EllipticCurveGroup::secp224r1();

		let msg = hex::decode("E5D5A7ADF73C5476FAEE93A2C76CE94DC0557DB04CDC189504779117920B896D").unwrap();
		let r = BigUint::from_be_bytes(&hex::decode("8163E5941BED41DA441B33E653C632A55A110893133351E20CE7CB75").unwrap());
		let s = BigUint::from_be_bytes(&hex::decode("D12C3FC289DDD5F6890DCE26B65792C8C50E68BF551D617D47DF15A8").unwrap());

		let sig = crate::x509::asn::PKIX1Algorithms2008::ECDSA_Sig_Value {
			r: r.into(),
			s: s.into()
		}.to_der();


		let point = hex::decode("04C5C9B38D3603FCCD6994CBB9594E152B658721E483669BB42728520F484B537647EC816E58A8284D3B89DFEDB173AFDC214ECA95A836FA7C").unwrap();

		let mut hasher = crate::sha256::SHA256Hasher::default();

		assert!(curve.verify_signature(&point, &sig, &msg, &mut hasher).unwrap());

	}


}



