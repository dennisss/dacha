use crate::big::secure::montgomery::SecureMontgomeryModulo;
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
pub struct SecureModulo<'a> {
    pub n: &'a SecureBigUint,
}

impl<'a> SecureModulo<'a> {
    pub fn new(n: &'a SecureBigUint) -> Self {
        SecureModulo { n }
    }

    pub fn rem(&self, a: &SecureBigUint) -> SecureBigUint {
        a % self.n
    }

    // Assuming the provided values are already in the space, we can preform much
    // cheaper addition correction.
    //
    // TODO: Perform add with carry here similar to
    // done in BearSSL to avoid having an extra bit.
    pub fn add(&self, a: &SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        // (a + b) % self.n
        let mut result = a + b;
        result.reduce_once(&self.n);
        result
    }

    pub fn add_into(&self, mut a: SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        a += b;
        a % self.n
    }

    pub fn sub(&self, a: &SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        // ((a + self.n) - b) % self.n
        let mut result = (a + self.n) - b;
        result.reduce_once(&self.n);
        result
    }

    // TODO: Even more efficient is b is also owned
    pub fn sub_into(&self, mut a: SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        a = a % self.n;
        a += self.n;
        a -= b % self.n;
        a = a % self.n;
        a
    }

    pub fn mul(&self, a: &SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        (a * b) % self.n
    }

    /// Computes a^b mod n
    pub fn pow(&self, a: &SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        let mont = SecureMontgomeryModulo::new(&self.n);

        let mut a_mont = a.clone();
        mont.to_montgomery_form(&mut a_mont);

        let result_mont = mont.pow(&a_mont, b);

        mont.from_montgomery_form(&result_mont)
    }

    /// Computes the modular inverse 'a^-1' such the 'a*(a^-1) = 1 mod n'.
    ///
    /// Algorithm is equivalent to the following (but using modular arithmetic
    /// instead of signed arithmetic): https://en.wikipedia.org/wiki/Extended_Euclidean_algorithm#Modular_integers
    pub fn inv(&self, a: &SecureBigUint) -> SecureBigUint {
        let mut t = SecureBigUint::from_usize(0, self.n.bit_width());
        let mut new_t = SecureBigUint::from_usize(1, self.n.bit_width());
        let mut r = self.n.clone();
        let mut new_r = a.clone();

        // TODO: This needs to use a fixed number of iterations.
        while !new_r.is_zero() {
            let (q, rem) = r.quorem(&new_r);
            tup!((t, new_t) = (new_t.clone(), self.sub(&t, &(&q * &new_t))));
            tup!((r, new_r) = (new_r.clone(), rem));
        }

        if r > SecureBigUint::from_usize(1, r.bit_width()) {
            panic!("Not invertible");
        }

        t
    }

    /// Computes '(a / b) mod n'.
    /// Internally performs '(a * b^-1) mod n'
    pub fn div(&self, a: &SecureBigUint, b: &SecureBigUint) -> SecureBigUint {
        self.mul(a, &self.inv(b))
    }

    /// Computes '-1*a mod n'
    pub fn negate(&self, a: &SecureBigUint) -> SecureBigUint {
        self.sub(self.n, a)
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
