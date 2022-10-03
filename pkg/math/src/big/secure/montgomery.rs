use crate::big::secure::modulo::SecureModulo;
use crate::big::secure::uint::*;
use crate::integer::Integer;

/// Context for performing modular arithmetic with a fixed modulus.
///
/// This assumes that the modulus is an odd number. This makes it easy to choose
/// a value R which is coprime to the modulus in constant time. In particular we
/// choose R to be base^n where 'n' is the number of base limbs in the modulus
/// storage.
///
/// All the same constraints apply as with the SecureModulus (integer operands
/// must be < the modulus).
///
/// In addition, using this requires that you:
/// - Convert all operands using to_montgomery_form().
/// - Perform all operations.
/// - Convert the result back to normal form using from_montgomery_form().
pub struct SecureMontgomeryModulo<'a> {
    modulus: &'a SecureBigUint,

    /// Number of bits in R where R = 2^(r_bits - 1) = b^n where 'b' is the
    /// maximum size of the limbs.
    r_bits: usize,

    /// -moduus^-1 mod base
    modulus_prime: BaseType,
}

impl<'a> SecureMontgomeryModulo<'a> {
    pub fn new(modulus: &'a SecureBigUint) -> Self {
        // Must be odd for us to be able to pick an R that is a power of 2 and still be
        // coprime with the modulus.
        assert!(modulus.value[0] % 2 == 1);

        let r_bits = modulus.bit_width() + 1;

        let modulus_prime = {
            // NOTE: This assumes the limb base is 2^32

            let mut inv = modulus.value[0]; // mod base
            let mut nbits = 2;
            while nbits < BASE_BITS {
                // inv = inv * (2 - m_0 * inv) mod base
                inv = inv.wrapping_mul(2u32.wrapping_sub(modulus.value[0].wrapping_mul(inv)));
                nbits *= 2;
            }

            ((inv as i32) * -1) as u32
        };

        Self {
            modulus,
            r_bits,
            modulus_prime,
        }
    }

    /// Computes 'a*R mod m'
    pub fn to_montgomery_form(&self, a: &mut SecureBigUint) {
        assert_eq!(a.bit_width(), self.modulus.bit_width());

        let mut tmp = SecureBigUint::from_usize(0, self.modulus.bit_width());

        for i in 1..self.r_bits {
            let carry = a.shl() != 0;
            let carry2 = a.overflowing_sub_to(&self.modulus, &mut tmp);

            // If true, after the shl, the current value is larger than the modulus.
            let overflowed_m = carry == carry2;

            // Set 'self = self - m' when we exceeded the modulus.
            tmp.copy_if(overflowed_m, a);
        }
    }

    pub fn from_montgomery_form(&self, t: &SecureBigUint) -> SecureBigUint {
        let one = SecureBigUint::from_usize(1, BASE_BITS);
        self.montgomery_mul(t, &one)
    }

    pub fn add(&self, a: &SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        SecureModulo::new(self.modulus).add(a, b)
    }

    pub fn sub(&self, a: &SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        SecureModulo::new(self.modulus).sub(a, b)
    }

    pub fn mul(&self, x: &SecureBigUint, y: &SecureBigUint) -> SecureBigUint {
        self.montgomery_mul(x, y)
    }

    /// Computes '(a^b) (R^(-1*b)) mod n'
    ///
    /// This means that 'a' should be in montgomery form and 'b' should be in
    /// normal form.
    ///
    /// Internally this is implemented using the 'double and add' algorithm.
    pub fn pow(&self, a: &SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        // 1 in montgomery form.
        let mut out = SecureBigUint::from_usize(1, self.modulus.bit_width());

        // TODO: If we want to convert the return value immediately to non-Montgomery
        // form after this operation, we can keep out in normal form to do that more
        // cheaply.
        //
        // TODO: Precompute this for the number 1.
        self.to_montgomery_form(&mut out);

        let mut p = a.clone();
        for i in 0..b.bit_width() {
            let next_out = self.mul(&out, &p);
            next_out.copy_if(b.bit(i) == 1, &mut out);

            p = self.mul(&p, &p);
        }

        out
    }

    /// Computes 'x*y*R^-1 mod m' using Montgomery reduction
    /// Algorithm 14.36 in the Handbook of Applied Cryptograph.
    fn montgomery_mul(&self, x: &SecureBigUint, y: &SecureBigUint) -> SecureBigUint {
        let mut a = SecureBigUint::from_usize(0, self.modulus.bit_width() + 2 * BASE_BITS);

        let n = self.modulus.value.len();
        for i in 0..n {
            // u_i = (a_0 + x_i y_0) m_prime mod base
            let u_i = (a.value[0].wrapping_add(x.value[i].wrapping_mul(y.value[0])))
                .wrapping_mul(self.modulus_prime);

            // TODO: Optimize out the memory allocations here.
            let x_i = SecureBigUint::from_usize(x.value[i] as usize, BASE_BITS);
            let u_i = SecureBigUint::from_usize(u_i as usize, BASE_BITS);

            // A = A + (x_i y) + (u_i m)
            x_i.add_mul_to(y, &mut a);
            u_i.add_mul_to(&self.modulus, &mut a);

            // A = A / base
            a.shr_base();
        }

        // If A >= m, A = A - m
        a.reduce_once(&self.modulus);

        a
    }
}
