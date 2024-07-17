use alloc::vec::Vec;

use crate::big::secure::modulo::SecureModulo;
use crate::big::secure::raw::{BaseType, SignedBaseType, BASE_BITS};
use crate::big::secure::storage::*;
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
pub struct SecureMontgomeryModulo<'a, SM: StorageType = Vec<BaseType>> {
    modulus: &'a SecureBigUint<SM>,

    /// Number of bits in R where R = 2^(r_bits - 1) = b^n where 'b' is the
    /// maximum size of the limbs.
    r_bits: usize,

    /// -modulus^-1 mod base
    modulus_prime: BaseType,
}

impl<'a, SM: StorageType> SecureMontgomeryModulo<'a, SM> {
    pub fn new(modulus: &'a SecureBigUint<SM>) -> Self {
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
                inv = inv
                    .wrapping_mul((2 as BaseType).wrapping_sub(modulus.value[0].wrapping_mul(inv)));
                nbits *= 2;
            }

            ((inv as SignedBaseType) * -1) as BaseType
        };

        Self {
            modulus,
            r_bits,
            modulus_prime,
        }
    }

    pub fn modulus(&self) -> &SecureBigUint<SM> {
        &self.modulus
    }

    /// Computes 'a*R mod m'
    pub fn to_montgomery_form<'b, A: Allocator<'b>, S: StorageTypeMut>(
        &self,
        a: &mut SecureBigUint<S>,
        allocator: &mut A,
    ) {
        assert_eq!(a.bit_width(), self.modulus.bit_width());

        let allocator = allocator.sub_allocator();

        let mut tmp = SecureBigUint::from_usize(0, self.modulus.bit_width(), &allocator);

        for i in 1..self.r_bits {
            let carry = a.shl() != 0;
            let carry2 = a.overflowing_sub_to(&self.modulus, &mut tmp);

            // If true, after the shl, the current value is larger than the modulus.
            let overflowed_m = carry == carry2;

            // Set 'self = self - m' when we exceeded the modulus.
            tmp.copy_if(overflowed_m, a);
        }
    }

    pub fn from_montgomery_form<'b, A: Allocator<'b>, S: StorageType>(
        &self,
        t: &SecureBigUint<S>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        let one = SecureBigUint::from_constant(1);
        self.montgomery_mul(t, &one, allocator)
    }

    pub fn add<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        SecureModulo::new(self.modulus).add(a, b, allocator)
    }

    pub fn sub<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        SecureModulo::new(self.modulus).sub(a, b, allocator)
    }

    pub fn mul<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        x: &SecureBigUint<S>,
        y: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        self.montgomery_mul(x, y, allocator)
    }

    /// Computes '(a^b) (R^(-1*b)) mod n'
    ///
    /// This means that 'a' should be in montgomery form and 'b' should be in
    /// normal form.
    ///
    /// Internally this is implemented using the 'double and add' algorithm.
    pub fn pow<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        // 1 in montgomery form.
        let mut out = SecureBigUint::from_usize(1, self.modulus.bit_width(), allocator);

        // TODO: If we want to convert the return value immediately to non-Montgomery
        // form after this operation, we can keep out in normal form to do that more
        // cheaply.
        //
        // TODO: Precompute this for the number 1.
        self.to_montgomery_form(&mut out, allocator);

        let mut p = a.clone_with(allocator);
        for i in 0..b.bit_width() {
            let next_out = self.mul(&out, &p, allocator);
            next_out.copy_if(b.bit(i) == 1, &mut out);

            p = self.mul(&p, &p, allocator);
        }

        out
    }

    /// An optimized version of pow() which is secure only if 'b' is a publicly
    /// known value.
    pub fn pow_with_public_exponent<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        // 1 in montgomery form.
        let mut out = SecureBigUint::from_usize(1, self.modulus.bit_width(), allocator);
        self.to_montgomery_form(&mut out, allocator);

        let mut p = a.clone_with(allocator);
        for i in 0..b.value_bits() {
            if b.bit(i) == 1 {
                out = self.mul(&out, &p, allocator);
            }

            // TODO: Only do this if we
            p = self.mul(&p, &p, allocator);
        }

        out
    }

    /// Computes 'x*y*R^-1 mod m' using Montgomery reduction
    /// Algorithm 14.36 in the Handbook of Applied Cryptograph.
    fn montgomery_mul<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        x: &SecureBigUint<S>,
        y: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        let mut a =
            SecureBigUint::from_usize(0, self.modulus.bit_width() + 2 * BASE_BITS, allocator);

        let n = self.modulus.value.as_ref().len();
        for i in 0..n {
            // u_i = (a_0 + x_i y_0) m_prime mod base
            let u_i = (a.value[0].wrapping_add(x.value[i].wrapping_mul(y.value[0])))
                .wrapping_mul(self.modulus_prime);

            let scope = allocator.sub_allocator();

            let x_i = SecureBigUint::from_constant(x.value[i] as usize);
            let u_i = SecureBigUint::from_constant(u_i as usize);

            // A = A + (x_i y) + (u_i m)
            x_i.add_mul_to(y, &mut a);
            u_i.add_mul_to(&self.modulus, &mut a);

            // A = A / base
            a.shr_base();
        }

        // If A >= m, A = A - m
        a.reduce_once(&self.modulus, &allocator.sub_allocator());

        a
    }

    /// Assuming the modulus is a prime number and 'a' < the modulus, this
    /// computes the 'a^-1 R^-1 mod m' efficiently.
    ///
    /// This uses Fermat's little theorem which says
    /// That: 'a^(m-1) = 1 mod m'
    /// So 'a * a^(m-2) = 1 mod p'
    /// So 'a^(m-2)' is the inverse
    ///
    /// TODO: If the prime is public knowledge and has sparse bits, it will be
    /// much more efficient to use 'pow_with_public_exponent'
    pub fn inv_prime_mod<'b, A: Allocator<'b>, S: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        // Note that all the primes we are dealing with are >2, so should subtraction
        // never overflow in practice. TODO: Pre-compute this.
        let two = SecureBigUint::from_constant(2);
        let exp = self.modulus.sub(&two, allocator);

        // TODO: Verify that we use the public exponent version here all the time.
        self.pow(a, &exp, allocator)
    }

    /// Similar to 'inv_prime_mod' except assumes that the modulus is public
    /// knowledge. This is usually much faster than 'inv_prime_mod'.
    pub fn inv_public_prime_mod<'b, A: Allocator<'b>, S: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        let two = SecureBigUint::from_constant(2);
        let exp = self.modulus.sub(&two, allocator);
        self.pow_with_public_exponent(a, &exp, allocator)
    }
}
