use crate::big_number::*;
use crate::random::*;
use crate::dh::*;
use common::errors::*;
use common::ceil_div;
use async_trait::async_trait;

/// Parameters of an elliptic curve of the form:
/// y^2 = x^3 + a*x + b
pub struct EllipticCurve {
	pub a: BigUint,
	pub b: BigUint
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

		// Output with padding.
		let mut out = vec![];
		out.resize(self.p.min_bytes(), 0);

		let data = n.to_be_bytes();
		let out_len = out.len();
		out[(out_len - data.len())..].copy_from_slice(&data);
		Ok(out)
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

impl EllipticCurveGroup {

	fn decode_scalar(&self, data: &[u8]) -> Result<BigUint> {
		if data.len() != self.p.min_bytes() {
			return Err("Scalar wrong size".into());
		}

		Ok(BigUint::from_be_bytes(data))
	}

	fn decode_point(&self, data: &[u8]) -> Result<EllipticCurvePoint> {
		if data.len() > 1 {
			return Err("Point too small".into());
		}

		let nbytes = ceil_div(self.p.nbits(), 8);

		let p = if data[0] == 4 {
			// Uncompressed form
			// TODO: For TLS 1.3, this is the only supported format
			if data.len() != 1 + 2*nbytes {
				return Err("Point data too small".into());
			}
			
			// TODO: 
			let x = BigUint::from_be_bytes(&data[1..nbytes]);
			let y = BigUint::from_be_bytes(&data[nbytes..]);

			EllipticCurvePoint { x, y }
		} else if data[0] == 2 || data[0] == 3 {
			// Compressed form.
			// Contains only X, data[0] contains the LSB of Y.
			if data.len() != 1 + nbytes {
				return Err("Point data too small".into());
			}

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
				y = Modulo::new(&self.p).negate(&y);
			}

			EllipticCurvePoint { x, y }
		} else {
			return Err("Unknown point format".into());
		};

		if !self.verify_point(&p) {
			return Err("Invalid point".into());
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
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFF0000000000000000FFFFFFFF",
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEFFFFFFFF0000000000000000FFFFFFFC",
			"B3312FA7E23EE7E4988E056BE3F82D19181D9C6EFE8141120314088F5013875AC656398D8A2ED19D2A85C8EDD3EC2AEF",
			"AA87CA22BE8B05378EB1C71EF320AD746E1D3B628BA79B9859F741E082542A385502F25DBF55296C3A545E3872760AB7",
			"3617DE4A96262C6F5D9E98BF9292DC29F8F41DBD289A147CE9DA3113B5F0B8C00A60B1CE1D7E819D7A431D7C90EA0E5F",
			"FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC7634D81F4372DDF581A0DB248B0A77AECEC196ACCC52973",
			1)
	}

	pub fn secp521r1() -> Self {
		Self::from_hex(
			"01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF",
			"01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFC",
			"0051953EB9618E1C9A1F929A21A0B68540EEA2DA725B99B315F3B8B489918EF109E156193951EC7E937B1652C0BD3BB1BF073573DF883D2C34F1EF451FD46B503F00",
			"00C6858E06B70404E9CD9E3ECB662395B4429C648139053FB521F828AF606B4D3DBAA14B5E77EFE75928FE1DC127A2FFA8DE3348B3C1856A429BF97E7E31C2E5BD66",
			"011839296A789A3BC0045C8A5FB42C7D1BD998F54449579B446817AFBD17273E662C97EE72995EF42640C550B9013FAD0761353C7086A272C24088BE94769FD16650",
			"01FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFA51868783BF2F966B7FCC0148F709A5D03BB5C9B8899C47AEBB6FB71E91386409",
			1)
	}



	/// Returns whether or not the given point is on the curve.
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
		(lhs % &self.n) == (rhs % &self.n)
	}

	/// See https://en.wikipedia.org/wiki/Elliptic_curve_point_multiplication#Point_addition
	/// This includes support for doubling points.
	fn add_points(&self, p: &EllipticCurvePoint,
				  q: &EllipticCurvePoint) -> EllipticCurvePoint {
		if p.is_inf() {
			return q.clone();
		}
		if q.is_inf() {
			return p.clone();
		}

		let m = Modulo::new(&self.p);
		let s = if p == q {
			m.div(&m.add_into(m.mul(&3.into(), &m.pow(&p.x, &2.into())),
						 &self.curve.a),
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
	pub fn curve_mul(&self, d: &BigUint) -> EllipticCurvePoint {
		// let mut n = self.g.clone();
		let mut q = EllipticCurvePoint::zero();
		for i in (0..d.nbits()).rev() {
			q = self.add_points(&q, &q);
			if d.bit(i) == 1 {
				q = self.add_points(&q, &self.g);
			}
		}

		q
	}

}

// TODO: Don't forget to mask the last bit when decoding k or u?

// TODO: Now all of these are using modular arithmetic right now

// TODO: See https://en.wikipedia.org/wiki/Exponentiation_by_squaring#Montgomery's_ladder_technique as a method of doing power operations in constant time

// TODO: See start of https://tools.ietf.org/html/rfc7748#section-5 for the encode/decode functions


// pub fn decode_u_cord()

// TODO: Need a custom function for sqr (aka n^2)

// 32 bytes for X25519 and 56 bytes for X448


pub struct MontgomeryCurveGroup {
	p: BigUint,
	bits: usize,
	a24: BigUint
}


pub struct X25519 {}

impl X25519 {
	/// Creates a new 32byte private key
	/// This will be the 'k'/scalar used to multiple the base point.
	pub async fn private_key() -> Result<Vec<u8>> {
		let mut data = vec![];
		data.resize(32, 0);
		secure_random_bytes(&mut data).await?;
		Ok(data)
	}

	/// Generates the public key associated with the given private key.
	pub fn public_key(private_key: &[u8]) -> Vec<u8> {
		let u = BigUint::from(9); // Base point
		let k = Self::decode_scalar(private_key);
		let out = Self::mul(&k, &u);
		Self::encode_u_cord(&out)
	}

	pub fn shared_secret(public_key: &[u8], private_key: &[u8]) -> Vec<u8> {
		let u = Self::decode_u_cord(public_key);
		let k = Self::decode_scalar(private_key);
		let out = Self::mul(&k, &u);
		Self::encode_u_cord(&u)
	}

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
		BigUint::from_le_bytes(data)
	}

	fn encode_u_cord(u: &BigUint) -> Vec<u8> {
		let mut data = u.to_le_bytes();
		assert!(data.len() <= 32);
		data.resize(32, 0);
		data
	}

	fn mul(k: &BigUint, u: &BigUint) -> BigUint {
		let p = BigUint::from(255).exp2() - &19.into();
		let bits = 255;
		let a24 = BigUint::from(121665);
		curve_mul(k, u, &p, bits, &a24)
	}
}

pub struct X448 {}

pub fn x448(k: &BigUint, u: &BigUint) -> BigUint {
	let p = BigUint::from(448).exp2()
		- BigUint::from(224).exp2() - BigUint::from(1);
	let bits = 448;
	let a24 = BigUint::from(39081);
	curve_mul(k, u, &p, bits, &a24)
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

	#[test]
	fn secp256r1_test() {
		{
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

		{
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
	}

	#[test]
	fn x25519_test() {
		assert_eq!(BigUint::from_le_bytes(
			&hex::decode("01").unwrap()).to_string(), "1");
		assert_eq!(BigUint::from_le_bytes(
			&hex::decode("0100000002").unwrap()).to_string(), "8589934593");

		let mut sdata = hex::decode(
			"a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4").unwrap();
		sdata[0] &= 248;
		sdata[31] &= 127;
		sdata[31] |= 64;

		assert_eq!(BigUint::from_le_bytes(&sdata).to_string(), "31029842492115040904895560451863089656472772604678260265531221036453811406496");

		let scalar = BigUint::from_str("31029842492115040904895560451863089656472772604678260265531221036453811406496").unwrap();
		let u_in = BigUint::from_str("34426434033919594451155107781188821651316167215306631574996226621102155684838").unwrap();

		let u_out = X25519::mul(&scalar, &u_in);
		assert_eq!(hex::encode(u_out.to_le_bytes()), "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552");
	}

}



