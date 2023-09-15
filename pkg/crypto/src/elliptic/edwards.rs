use alloc::vec::Vec;

use common::errors::*;
use math::big::*;
use math::Integer;

use crate::hasher::Hasher;
use crate::random::SharedRng;
use crate::sha512::SHA512Hasher;

// TODO: Figure out how to best re-use code with the montgomery curve code.

/*
https://www.rfc-editor.org/rfc/rfc8032
*/

/// Group of (x, y) points defined over an elliptic curve of the form:
/// 'a x^2 + y^2 = 1 + d*x^2*y^2' where points are calculated 'mod p'.
///
/// See also https://en.wikipedia.org/wiki/Twisted_Edwards_curve.
pub struct EdwardsCurveGroup {
    p: SecureBigUint,

    a: SecureBigUint,

    d: SecureBigUint,

    /// Base point on the curve. All other points are calculated as a multiple
    /// of this one.
    base_point: EdwardsCurvePoint,

    order: SecureBigUint,

    /// For EdDSA, this is the number of bits used to encode integers and
    /// points.
    ///
    /// Also known as 'b' in RFC 8032.
    ///
    /// MUST be divisible by 8 so that whole octets can be used.
    /// Integers in the range [0, p) MUST be representable with 'b - 1' bits.
    /// The hash function must be able to generate '2*b' bits.
    encoding_bits: usize,
}

impl EdwardsCurveGroup {
    pub fn ed25519() -> Self {
        let bits = 255;
        let p = {
            let working_bits = 256;
            let mut v = SecureBigUint::exp2(255, working_bits)
                - SecureBigUint::from_usize(19, working_bits);
            v.truncate(bits);
            v
        };

        // -1
        let a = SecureModulo::new(&p).negate(&SecureBigUint::from_usize(1, bits));

        //-121665/121666
        let d = SecureBigUint::from_str(
            "37095705934669439343138083508754565189542113879843219016388785533085940283555",
            bits,
        )
        .unwrap();

        let mut order = SecureBigUint::exp2(252, bits)
            + &SecureBigUint::from_str("27742317777372353535851937790883648493", bits).unwrap();
        order.truncate(253);

        let base_point = EdwardsCurvePoint {
            x: SecureBigUint::from_str(
                "15112221349535400772501151409588531511454012693041857206046113283949847762202",
                bits,
            )
            .unwrap(),
            y: SecureBigUint::from_str(
                "46316835694926478169428394003475163141307993866256225615783033603165251855960",
                bits,
            )
            .unwrap(),
        };

        Self {
            p,
            a,
            d,
            order,
            base_point,
            encoding_bits: 256,
        }
    }

    fn encoding_bytes(&self) -> usize {
        self.encoding_bits / 8
    }

    pub async fn generate_private_key(&self) -> Vec<u8> {
        let mut key = vec![0; self.encoding_bytes()];
        crate::random::global_rng().generate_bytes(&mut key).await;
        key
    }

    /// Expands a private key to a public key which can be used to
    pub fn public_key(&self, private_key: &[u8]) -> Result<Vec<u8>> {
        let (secret_scalar, prefix) = self.expand_private_key(private_key)?;
        let public_value = self.scalar_mul_point(&secret_scalar, &self.base_point);
        let public_key = self.encode_point(&public_value);
        Ok(public_key)
    }

    /// See RFC 8032
    /// TODO: Also support
    pub fn create_signature(&self, private_key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
        let (secret_scalar, prefix) = self.expand_private_key(private_key)?;

        let public_value = self.scalar_mul_point(&secret_scalar, &self.base_point);

        let public_key = self.encode_point(&public_value);

        let r = {
            let mut hasher = SHA512Hasher::default();
            hasher.update(&prefix);
            let hash = hasher.finish_with(data);

            SecureBigUint::from_le_bytes(&hash) % &self.order
        };

        let r_s = self.encode_point(&self.scalar_mul_point(&r, &self.base_point));

        let mut s = {
            let h = {
                let mut hasher = SHA512Hasher::default();
                hasher.update(&r_s);
                hasher.update(&public_key);
                let hash = hasher.finish_with(data);

                SecureBigUint::from_le_bytes(&hash) % &self.order
            };

            (r + &(h * &secret_scalar)) % &self.order
        };

        let mut out = vec![];
        out.extend_from_slice(&r_s);
        out.extend_from_slice(&s.to_le_bytes());

        Ok(out)
    }

    pub fn verify_signature(
        &self,
        public_key: &[u8],
        signature: &[u8],
        data: &[u8],
    ) -> Result<bool> {
        let public_point = self.decode_point(&public_key)?;

        let n = self.encoding_bytes();
        if signature.len() != 2 * n {
            return Err(err_msg("Invalid signature length"));
        }

        let r_s = &signature[0..n];
        let r = self.decode_point(r_s)?;

        let s = SecureBigUint::from_le_bytes(&signature[n..]);
        if s >= self.order {
            return Err(err_msg("Invalid signature"));
        }

        let h = {
            let mut hasher = SHA512Hasher::default();
            hasher.update(r_s);
            hasher.update(public_key);
            let hash = hasher.finish_with(data);

            SecureBigUint::from_le_bytes(&hash) % &self.order
        };

        let s_b = self.scalar_mul_point(&s, &self.base_point);
        let h_a = self.scalar_mul_point(&h, &public_point);

        // 'R + hA'
        let expected = {
            let mut modulo = SecureMontgomeryModulo::new(&self.p);
            let mut r = r.to_projective();
            let mut h_a = h_a.to_projective();
            modulo.to_montgomery_form(&mut r.x);
            modulo.to_montgomery_form(&mut r.y);
            modulo.to_montgomery_form(&mut r.z);
            modulo.to_montgomery_form(&mut h_a.x);
            modulo.to_montgomery_form(&mut h_a.y);
            modulo.to_montgomery_form(&mut h_a.z);

            let out = self.add_points(&r, &h_a, &modulo);

            let p = EdwardsCurvePoint::from_projective(&out, &modulo);

            EdwardsCurvePoint {
                x: modulo.from_montgomery_form(&p.x),
                y: modulo.from_montgomery_form(&p.y),
            }
        };

        Ok(s_b.x == expected.x && s_b.y == expected.y)
    }

    /*
    def sha512_modq(s):
        return int.from_bytes(sha512(s), "little") % q
    */

    /*
    def secret_to_public(secret):
        (a, dummy) = secret_expand(secret)
        return point_compress(point_mul(a, G))
    */

    fn expand_private_key(&self, private_key: &[u8]) -> Result<(SecureBigUint, Vec<u8>)> {
        let n = self.encoding_bytes();
        if private_key.len() != n {
            return Err(err_msg("Unsupported private key size"));
        }

        // TODO: Use a differnet hasher depending on curve type.
        let mut h = SHA512Hasher::default().finish_with(private_key);
        assert_eq!(h.len(), 2 * n);

        // TODO: Generalize this to support x448 as well.
        h[0] &= !0b111; // Clear lower 3 bits.
        h[31] &= !(1 << 7); // Clear highest bit of last octet
        h[31] |= (1 << 6); // Set the second highest bit of last octet.

        Ok((SecureBigUint::from_le_bytes(&h[0..n]), h[n..].to_vec()))
    }

    /// Points are encoded as the 'y' coordinate in little endian with the top
    /// most bit set to the 'sign' (LSB) of the 'x' coordinate.
    fn encode_point(&self, point: &EdwardsCurvePoint) -> Vec<u8> {
        let n = self.encoding_bytes();
        let mut data = point.y.to_le_bytes();
        data.resize(n, 0);
        data[n - 1] |= (point.x.bit(0) as u8) << 7;
        data
    }

    fn decode_point(&self, data: &[u8]) -> Result<EdwardsCurvePoint> {
        let n = self.encoding_bytes();
        if n != data.len() {
            return Err(err_msg("Encoded point is wrong size"));
        }

        let mut data = data.to_vec();

        // Extract and remove the sign of the x coordinate
        let x_sign = data[data.len() - 1] >> 7; // Upper bit

        let n = data.len();
        data[n - 1] &= !(1 << 7);

        let mut y = SecureBigUint::from_le_bytes(&data);
        if y >= self.p {
            return Err(err_msg("Y coordinate out of range"));
        }
        y.truncate(self.p.bit_width());

        // 'x^2 = (1 - y^2) / (a - d*y^2)'
        let x2 = {
            let modulo = SecureMontgomeryModulo::new(&self.p);

            let mut one = SecureBigUint::from_usize(1, self.p.bit_width());
            let mut y = y.clone();
            let mut a = self.a.clone();
            let mut d = self.d.clone();

            modulo.to_montgomery_form(&mut one);
            modulo.to_montgomery_form(&mut y);
            modulo.to_montgomery_form(&mut a);
            modulo.to_montgomery_form(&mut d);

            let y2 = modulo.mul(&y, &y);

            let numerator = modulo.sub(&one, &y2);
            let denominator = modulo.sub(&a, &modulo.mul(&d, &y2));

            let res = modulo.mul(&numerator, &modulo.inv_prime_mod(&denominator));

            modulo.from_montgomery_form(&res)
        };

        let mut x = match SecureModulo::new(&self.p).isqrt(&x2) {
            Some(v) => v,
            None => return Err(err_msg("X has no root")),
        };

        if (x.bit(0) as u8) != x_sign {
            x = SecureModulo::new(&self.p).negate(&x);
        }

        // Should only be possible if x = 0, but a sign of '1' was encoded.
        if (x.bit(0) as u8) != x_sign {
            return Err(err_msg("Invalid sign passed"));
        }

        Ok(EdwardsCurvePoint { x, y })
    }

    fn scalar_mul_point(&self, s: &SecureBigUint, p: &EdwardsCurvePoint) -> EdwardsCurvePoint {
        let mut modulo = SecureMontgomeryModulo::new(&self.p);

        let mut p = p.to_projective();
        modulo.to_montgomery_form(&mut p.x);
        modulo.to_montgomery_form(&mut p.y);
        modulo.to_montgomery_form(&mut p.z);

        let out = self.scalar_mul_point_inner(s, &p, &modulo);

        let out = EdwardsCurvePoint::from_projective(&out, &modulo);

        EdwardsCurvePoint {
            x: modulo.from_montgomery_form(&out.x),
            y: modulo.from_montgomery_form(&out.y),
        }
    }

    fn scalar_mul_point_inner(
        &self,
        s: &SecureBigUint,
        p: &EdwardsCurveProjectivePoint,
        modulo: &SecureMontgomeryModulo,
    ) -> EdwardsCurveProjectivePoint {
        let mut r_0 = EdwardsCurvePoint::neutral(&self.p).to_projective();
        modulo.to_montgomery_form(&mut r_0.x);
        modulo.to_montgomery_form(&mut r_0.y);
        modulo.to_montgomery_form(&mut r_0.z);

        let mut r_1 = p.clone();

        let mut swap = false;

        for i in (0..s.bit_width()).rev() {
            let s_i = s.bit(i) != 0;
            swap ^= s_i;

            r_0.x.swap_if(&mut r_1.x, swap);
            r_0.y.swap_if(&mut r_1.y, swap);
            r_0.z.swap_if(&mut r_1.z, swap);
            swap = s_i;

            r_1 = self.add_points(&r_0, &r_1, modulo);
            r_0 = self.add_points(&r_0, &r_0, modulo);
        }

        r_0.x.swap_if(&mut r_1.x, swap);
        r_0.y.swap_if(&mut r_1.y, swap);
        r_0.z.swap_if(&mut r_1.z, swap);

        r_0
    }

    fn add_points(
        &self,
        p: &EdwardsCurveProjectivePoint,
        q: &EdwardsCurveProjectivePoint,
        modulo: &SecureMontgomeryModulo,
    ) -> EdwardsCurveProjectivePoint {
        // A = Z1*Z2
        let a = modulo.mul(&p.z, &q.z);

        // B = A^2
        let b = modulo.mul(&a, &a);

        // C = X1*X2
        let c = modulo.mul(&p.x, &q.x);

        // D = Y1*Y2
        let d = modulo.mul(&p.y, &q.y);

        let mut self_d = self.d.clone();
        modulo.to_montgomery_form(&mut self_d);

        // E = d*C*D
        // TODO: Convert 'self.d' to montgomery form.
        let e = modulo.mul(&self_d, &modulo.mul(&c, &d));

        // F = B-E
        let f = modulo.sub(&b, &e);

        // G = B+E
        let g = modulo.add(&b, &e);

        // H = (X1+Y1)*(X2+Y2)
        let h = modulo.mul(&modulo.add(&p.x, &p.y), &modulo.add(&q.x, &q.y));

        // X3 = A*F*(H-C-D)
        let x = {
            let tmp = modulo.sub(&h, &modulo.add(&c, &d));
            modulo.mul(&a, &modulo.mul(&f, &tmp))
        };

        // Y3 = A*G*(D-aC)
        let y = {
            let mut self_a = self.a.clone();
            modulo.to_montgomery_form(&mut self_a);

            // TODO: Convert 'self.a' into montgomery form.
            let tmp = modulo.sub(&d, &modulo.mul(&self_a, &c));
            modulo.mul(&a, &modulo.mul(&g, &tmp))
        };

        // Z3 = F*G
        let z = modulo.mul(&f, &g);

        EdwardsCurveProjectivePoint { x, y, z }
    }
}

#[derive(Clone)]
struct EdwardsCurvePoint {
    x: SecureBigUint,
    y: SecureBigUint,
}

impl EdwardsCurvePoint {
    fn neutral(p: &SecureBigUint) -> Self {
        Self {
            x: SecureBigUint::from_usize(0, p.bit_width()),
            y: SecureBigUint::from_usize(1, p.bit_width()),
        }
    }

    fn to_projective(&self) -> EdwardsCurveProjectivePoint {
        EdwardsCurveProjectivePoint {
            x: self.x.clone(),
            y: self.y.clone(),
            z: SecureBigUint::from_usize(1, self.x.bit_width()),
        }
    }

    /// x = X/Z, y = Y/Z
    fn from_projective(
        point: &EdwardsCurveProjectivePoint,
        modulo: &SecureMontgomeryModulo,
    ) -> Self {
        // TODO: Use fast inverse for prime modulus/
        let z_inv = modulo.inv_prime_mod(&point.z);

        Self {
            x: modulo.mul(&point.x, &z_inv),
            y: modulo.mul(&point.y, &z_inv),
        }
    }

    fn to_extended(&self, p: &SecureBigUint) -> EdwardsCurveExtendedPoint {
        EdwardsCurveExtendedPoint {
            x: self.x.clone(),
            y: self.y.clone(),
            z: SecureBigUint::from_usize(1, p.bit_width()),
            t: SecureModulo::new(p).mul(&self.x, &self.y),
        }
    }

    /// x = X/Z, y = Y/Z, x * y = T/Z
    fn from_extended(point: &EdwardsCurveExtendedPoint, p: &SecureBigUint) -> Self {
        let modulo = SecureModulo::new(p);

        // TODO: Use fast inverse for prime modulus/
        let z_inv = modulo.inv(&point.z);

        Self {
            x: modulo.mul(&point.x, &z_inv),
            y: modulo.mul(&point.y, &z_inv),
        }
    }
}

#[derive(Clone)]
struct EdwardsCurveProjectivePoint {
    x: SecureBigUint,
    y: SecureBigUint,
    z: SecureBigUint,
}

#[derive(Clone)]
struct EdwardsCurveExtendedPoint {
    x: SecureBigUint,
    y: SecureBigUint,
    z: SecureBigUint,
    t: SecureBigUint,
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCase {
        private_key: &'static [u8],
        public_key: &'static [u8],
        message: &'static [u8],
        signature: &'static [u8],
    }

    // Test vectors from RFC 8032 Section 7.1
    #[test]
    fn ed25519_signature_test() {
        let test_cases = &[
            TestCase {
                private_key: &hex!("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60"),
                public_key: &hex!("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a"),
                message: b"",
                signature: &hex!("e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"),
            },

            TestCase {
                private_key: &hex!("4ccd089b28ff96da9db6c346ec114e0f5b8a319f35aba624da8cf6ed4fb8a6fb"),
                public_key: &hex!("3d4017c3e843895a92b70aa74d1b7ebc9c982ccf2ec4968cc0cd55f12af4660c"),
                message: &hex!("72"),
                signature: &hex!("92a009a9f0d4cab8720e820b5f642540a2b27b5416503f8fb3762223ebdb69da085ac1e43e15996e458f3613d0f11d8c387b2eaeb4302aeeb00d291612bb0c00"),
            },

            TestCase {
                private_key: &hex!("c5aa8df43f9f837bedb7442f31dcb7b1
                66d38535076f094b85ce3a2e0b4458f7"),
                public_key: &hex!("fc51cd8e6218a1a38da47ed00230f058
                0816ed13ba3303ac5deb911548908025"),
                message: &hex!("af82"),
                signature: &hex!("6291d657deec24024827e69c3abe01a3
                0ce548a284743a445e3680d7db5ac3ac
                18ff9b538d16f290ae67f760984dc659
                4a7c15e9716ed28dc027beceea1ec40a"),
            },

            TestCase {
                private_key: &hex!("f5e5767cf153319517630f226876b86c
                8160cc583bc013744c6bf255f5cc0ee5"),
                public_key: &hex!("278117fc144c72340f67d0f2316e8386
                ceffbf2b2428c9c51fef7c597f1d426e"),
                message: &hex!("08b8b2b733424243760fe426a4b54908
                632110a66c2f6591eabd3345e3e4eb98
                fa6e264bf09efe12ee50f8f54e9f77b1
                e355f6c50544e23fb1433ddf73be84d8
                79de7c0046dc4996d9e773f4bc9efe57
                38829adb26c81b37c93a1b270b20329d
                658675fc6ea534e0810a4432826bf58c
                941efb65d57a338bbd2e26640f89ffbc
                1a858efcb8550ee3a5e1998bd177e93a
                7363c344fe6b199ee5d02e82d522c4fe
                ba15452f80288a821a579116ec6dad2b
                3b310da903401aa62100ab5d1a36553e
                06203b33890cc9b832f79ef80560ccb9
                a39ce767967ed628c6ad573cb116dbef
                efd75499da96bd68a8a97b928a8bbc10
                3b6621fcde2beca1231d206be6cd9ec7
                aff6f6c94fcd7204ed3455c68c83f4a4
                1da4af2b74ef5c53f1d8ac70bdcb7ed1
                85ce81bd84359d44254d95629e9855a9
                4a7c1958d1f8ada5d0532ed8a5aa3fb2
                d17ba70eb6248e594e1a2297acbbb39d
                502f1a8c6eb6f1ce22b3de1a1f40cc24
                554119a831a9aad6079cad88425de6bd
                e1a9187ebb6092cf67bf2b13fd65f270
                88d78b7e883c8759d2c4f5c65adb7553
                878ad575f9fad878e80a0c9ba63bcbcc
                2732e69485bbc9c90bfbd62481d9089b
                eccf80cfe2df16a2cf65bd92dd597b07
                07e0917af48bbb75fed413d238f5555a
                7a569d80c3414a8d0859dc65a46128ba
                b27af87a71314f318c782b23ebfe808b
                82b0ce26401d2e22f04d83d1255dc51a
                ddd3b75a2b1ae0784504df543af8969b
                e3ea7082ff7fc9888c144da2af58429e
                c96031dbcad3dad9af0dcbaaaf268cb8
                fcffead94f3c7ca495e056a9b47acdb7
                51fb73e666c6c655ade8297297d07ad1
                ba5e43f1bca32301651339e22904cc8c
                42f58c30c04aafdb038dda0847dd988d
                cda6f3bfd15c4b4c4525004aa06eeff8
                ca61783aacec57fb3d1f92b0fe2fd1a8
                5f6724517b65e614ad6808d6f6ee34df
                f7310fdc82aebfd904b01e1dc54b2927
                094b2db68d6f903b68401adebf5a7e08
                d78ff4ef5d63653a65040cf9bfd4aca7
                984a74d37145986780fc0b16ac451649
                de6188a7dbdf191f64b5fc5e2ab47b57
                f7f7276cd419c17a3ca8e1b939ae49e4
                88acba6b965610b5480109c8b17b80e1
                b7b750dfc7598d5d5011fd2dcc5600a3
                2ef5b52a1ecc820e308aa342721aac09
                43bf6686b64b2579376504ccc493d97e
                6aed3fb0f9cd71a43dd497f01f17c0e2
                cb3797aa2a2f256656168e6c496afc5f
                b93246f6b1116398a346f1a641f3b041
                e989f7914f90cc2c7fff357876e506b5
                0d334ba77c225bc307ba537152f3f161
                0e4eafe595f6d9d90d11faa933a15ef1
                369546868a7f3a45a96768d40fd9d034
                12c091c6315cf4fde7cb68606937380d
                b2eaaa707b4c4185c32eddcdd306705e
                4dc1ffc872eeee475a64dfac86aba41c
                0618983f8741c5ef68d3a101e8a3b8ca
                c60c905c15fc910840b94c00a0b9d0"),
                signature: &hex!("0aab4c900501b3e24d7cdf4663326a3a
                87df5e4843b2cbdb67cbf6e460fec350
                aa5371b1508f9f4528ecea23c436d94b
                5e8fcd4f681e30a6ac00a9704a188a03"),
            },

            TestCase {
                private_key: &hex!("833fe62409237b9d62ec77587520911e
                9a759cec1d19755b7da901b96dca3d42"),
                public_key: &hex!("ec172b93ad5e563bf4932c70e1245034
                c35467ef2efd4d64ebf819683467e2bf"),
                message: &hex!("ddaf35a193617abacc417349ae204131
                12e6fa4e89a97ea20a9eeee64b55d39a
                2192992a274fc1a836ba3c23a3feebbd
                454d4423643ce80e2a9ac94fa54ca49f"),
                signature: &hex!("dc2a4459e7369633a52b1bf277839a00
                201009a3efbf3ecb69bea2186c26b589
                09351fc9ac90b3ecfdfbc7c66431e030
                3dca179c138ac17ad9bef1177331a704"),
            },

        ];

        let g = EdwardsCurveGroup::ed25519();

        for test_case in test_cases {
            assert_eq!(
                g.create_signature(test_case.private_key, test_case.message)
                    .unwrap(),
                test_case.signature
            );

            assert_eq!(
                g.public_key(test_case.private_key).unwrap(),
                test_case.public_key
            );

            assert_eq!(
                g.verify_signature(test_case.public_key, test_case.signature, test_case.message)
                    .unwrap(),
                true
            );

            // TODO: Do smarter fuzzing. e.g. try the all zero signature, only half zero,
            // etc.
            let mut bad_signature = test_case.signature.to_vec();
            bad_signature[4] = bad_signature[4].wrapping_add(1);

            assert_eq!(
                g.verify_signature(test_case.public_key, &bad_signature, test_case.message)
                    .unwrap_or(false),
                false
            );

            let mut bad_signature2 = test_case.signature.to_vec();
            bad_signature2[51] = bad_signature2[51].wrapping_add(1);

            assert_eq!(
                g.verify_signature(test_case.public_key, &bad_signature2, test_case.message)
                    .unwrap_or(false),
                false
            );
        }
    }
}
