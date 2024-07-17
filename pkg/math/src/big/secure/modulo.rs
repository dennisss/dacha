use crate::big::secure::montgomery::SecureMontgomeryModulo;
use crate::big::secure::storage::*;
use crate::big::secure::uint::SecureBigUint;
use crate::integer::Integer;
use crate::number::{One, Zero};

/// Operations over the finite field of integers 'mod n'.
///
/// All methods assume that the inputs are of the same or smaller width of the
/// modulus and that the input values are in the range [0, n). If a number
/// doesn't fit this criteria, it can be reduced using rem().
///
/// - 'n' doesn't need to be prime, but needs to be odd for 'pow' to work.
/// - The output of all operations is a number in the range [0, n).
/// - If an output buffer isn't provided, an output buffer of the same size as
///   the modulus will be chosen.
pub struct SecureModulo<'a, SM: StorageType> {
    pub n: &'a SecureBigUint<SM>,
}

impl<'a, SM: StorageType> SecureModulo<'a, SM> {
    pub fn new(n: &'a SecureBigUint<SM>) -> Self {
        SecureModulo { n }
    }

    pub fn rem<'b, A: Allocator<'b>, S: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        let (_, r) = a.quorem(&self.n, allocator);
        r
        // a % self.n
    }

    // Assuming the provided values are already in the space, we can preform much
    // cheaper addition correction.
    //
    // TODO: Perform add with carry here similar to
    // done in BearSSL to avoid having an extra bit.
    pub fn add<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        // (a + b) % self.n
        let mut result = a.add(b, allocator);
        result.reduce_once(&self.n, allocator);
        result
    }

    /*
    // TODO: Maybe use reduce_once.
    pub fn add_into(&self, mut a: SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        a += b;
        a % self.n
    }
    */

    pub fn sub<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        // ((a + self.n) - b) % self.n
        let mut result = a.add(&self.n, allocator);
        result.sub_assign(b);
        result.reduce_once(&self.n, allocator);
        result
    }

    /*
    // TODO: Even more efficient is b is also owned
    pub fn sub_into(&self, mut a: SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        a = a % self.n;
        a += self.n;
        a -= b % self.n;
        a = a % self.n;
        a
    }
    */

    pub fn mul<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        // (a * b) % self.n

        let x = a.mul(b, allocator);
        let (_, r) = x.quorem(&self.n, allocator);
        r
    }

    /// Computes a^b mod n
    pub fn pow<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        let mont = SecureMontgomeryModulo::new(&self.n);

        let mut a_mont = a.clone_with(allocator);
        mont.to_montgomery_form(&mut a_mont, allocator);

        let result_mont = mont.pow(&a_mont, b, allocator);

        mont.from_montgomery_form(&result_mont, allocator)
    }

    /// Computes the modular inverse 'a^-1' such the 'a*(a^-1) = 1 mod n'.
    ///
    /// Algorithm is equivalent to the following (but using modular arithmetic
    /// instead of signed arithmetic): https://en.wikipedia.org/wiki/Extended_Euclidean_algorithm#Modular_integers
    pub fn inv<'b, A: Allocator<'b>, S: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        let mut t = SecureBigUint::from_usize(0, self.n.bit_width(), allocator);
        let mut new_t = SecureBigUint::from_usize(1, self.n.bit_width(), allocator);
        let mut r = self.n.clone_with(allocator);
        let mut new_r = a.clone_with(allocator);

        // TODO: This needs to use a fixed number of iterations.
        // XXX: Yes
        while !new_r.is_zero() {
            let (q, rem) = r.quorem(&new_r, allocator);
            tup!(
                (t, new_t) = (
                    new_t.clone_with(allocator),
                    self.sub(&t, &(q.mul(&new_t, allocator)), allocator)
                )
            );
            tup!((r, new_r) = (new_r.clone_with(allocator), rem));
        }

        if r > SecureBigUint::from_usize(1, r.bit_width(), allocator) {
            panic!("Not invertible");
        }

        t
    }

    /// If the number has an exact square root, returns one of them.
    pub fn isqrt<'b, A: Allocator<'b>, S: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        allocator: &mut A,
    ) -> Option<SecureBigUint<A::Storage>> {
        assert!(a < self.n);

        let candidate = {
            if self.n.mod_word() % 4 == 3 {
                // Algorithm 3.36 in 'Handbook of Applied Cryptography'
                // TODO: This requires that 'n' is also prime.

                // TODO: Use the publicly known exponent optimization
                // = a^((p + 1) / 4)

                let mut exp = self.n.add(&SecureBigUint::from_constant(1), allocator);
                exp.shr_n(2);

                self.pow(a, &exp, allocator)
            } else if self.n.mod_word() % 8 == 5 {
                // Algorithm 3.36 in 'Handbook of Applied Cryptography'

                let mut x = {
                    // x = a^((p + 3) / 8)
                    let mut exp = self.n.add(&SecureBigUint::from_constant(3), allocator);
                    exp.shr_n(3);

                    self.pow(a, &exp, allocator)
                };

                let mut x_valid = &self.mul(&x, &x, allocator) == a;

                // Alternative root is '2^((p-1)/4) * x'
                let x_alt = {
                    let mut exp = self.n.sub(&SecureBigUint::from_constant(1), allocator);
                    assert_eq!(exp.bit(0), 0);
                    assert_eq!(exp.bit(1), 0);
                    exp.shr_n(2);

                    let two_exp = self.pow(
                        &SecureBigUint::from_usize(2, self.n.bit_width(), allocator),
                        &exp,
                        allocator,
                    );

                    self.mul(&two_exp, &x, allocator)
                };

                x_alt.copy_if(!x_valid, &mut x);

                x
            } else {
                panic!("Generic isqrt not supported")
            }
        };

        if &self.mul(&candidate, &candidate, allocator) == a {
            Some(candidate)
        } else {
            None
        }
    }

    /// Computes '(a / b) mod n'.
    /// Internally performs '(a * b^-1) mod n'
    pub fn div<'b, A: Allocator<'b>, S: StorageType, S2: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        b: &SecureBigUint<S2>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        self.mul(a, &self.inv(b, allocator), allocator)
    }

    /// Computes '-1*a mod n'
    pub fn negate<'b, A: Allocator<'b>, S: StorageType>(
        &self,
        a: &SecureBigUint<S>,
        allocator: &mut A,
    ) -> SecureBigUint<A::Storage> {
        self.sub(self.n, a, allocator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modulo_test() {
        // let p = BigUint::from(7);
        // let m = Modulo::new(&p);
        // let x = m.inv(&2.into());
        // assert_eq!(x, BigUint::from(4));
        // assert_eq!(m.div(&1.into(), &2.into()), BigUint::from(4));
    }
}
