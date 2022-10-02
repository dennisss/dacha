use crate::big::secure::uint::*;
use crate::integer::Integer;

/// Context for performing modular arithmetic with a fixed modulus.
///
/// This assumes that the modulus is a prime number. Then we will internally
/// choose a value R which is coprime to the modulus which is base^n where 'n'
/// is the number of base limbs in the modulus storage.
///
/// To use this:
/// - Convert all operands using to_montgomery_form().
/// - Perform all operations.
/// - Convert the result back to normal form using from_montgomery_form().
pub struct SecureMontgomeryModulo<'a> {
    modulus: &'a SecureBigUint,

    /// Number of bits in R where R = 2^(r_bits - 1) = b^n where 'b' is the
    /// maximum size of the limbs.
    r_bits: usize,

    /// -m^-1 mod base
    modulus_prime: u32,
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

    pub fn mul(&self, x: &SecureBigUint, y: &SecureBigUint) -> SecureBigUint {
        self.montgomery_mul(x, y)
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
            {
                assert_eq!(a.value[0], 0);
                for j in 1..a.value.len() {
                    a.value[j - 1] = a.value[j];
                }
                let k = a.value.len();
                a.value[k - 1] = 0;
            }
        }

        // If A >= m, A = A - m
        // TODO: Deduplicate this logic everywhere.
        {
            // TODO: Avoid this copy with an overflowing_sub_to
            let mut tmp = a.clone();
            let carry = tmp.overflowing_sub_assign(&self.modulus);
            tmp.copy_if(!carry, &mut a);
        }

        // Truncate to the size of the modulus.
        a.truncate(self.modulus.bit_width());

        a
    }
}
