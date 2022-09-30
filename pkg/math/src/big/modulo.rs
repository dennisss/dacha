use crate::big::uint::BigUint;
use crate::integer::Integer;
use crate::number::{One, Zero};

/// A set of operations which all result in a 'mod n' result.
/// TODO: This would ideally implement operations which have intermediate
/// results bounded by the size of the modulus.
pub struct Modulo<'a> {
    pub n: &'a BigUint,
}

// sub_assign(self, rhs: &BigUint)
// sub_to(&self, rhs: &BigUint, out: &bigUint)

impl<'a> Modulo<'a> {
    pub fn new(n: &'a BigUint) -> Self {
        Modulo { n }
    }

    pub fn rem(&self, a: &BigUint) -> BigUint {
        a % self.n
    }

    pub fn add(&self, a: &BigUint, b: &BigUint) -> BigUint {
        (a + b) % self.n
    }

    pub fn add_into(&self, mut a: BigUint, b: &BigUint) -> BigUint {
        a += b;
        a % self.n
    }

    pub fn sub(&self, a: &BigUint, b: &BigUint) -> BigUint {
        (((a % self.n) + self.n) - (b % self.n)) % self.n
    }

    // TODO: Even more efficient is b is also owned
    pub fn sub_into(&self, mut a: BigUint, b: &BigUint) -> BigUint {
        a = a % self.n;
        a += self.n;
        a -= b % self.n;
        a = a % self.n;
        a
    }

    pub fn mul(&self, a: &BigUint, b: &BigUint) -> BigUint {
        (a * b) % self.n
    }

    /// Computes a^b mod n
    pub fn pow(&self, a: &BigUint, b: &BigUint) -> BigUint {
        if b.is_zero() {
            return BigUint::from(1);
        }
        if b.is_one() {
            return a.clone();
        }

        let mut out = BigUint::one();
        let mut p = a.clone();
        for i in 0..b.value_bits() {
            // TODO: Use a smart multiplication function that still reads the bytes from 'p'
            // but only multiplies by it if needed.
            if b.bit(i) == 1 {
                out = self.mul(&out, &p);
            }
            p = self.mul(&p, &p);
        }

        out
    }

    /// Computes the modular inverse 'a^-1' such the 'a*(a^-1) = 1 mod n'.
    ///
    /// Algorithm is equivalent to the following (but using modular arithmetic
    /// instead of signed arithmetic): https://en.wikipedia.org/wiki/Extended_Euclidean_algorithm#Modular_integers
    pub fn inv(&self, a: &BigUint) -> BigUint {
        let mut t = BigUint::zero();
        let mut new_t = BigUint::one();
        let mut r = self.n.clone();
        let mut new_r = a.clone();

        while !new_r.is_zero() {
            let (q, rem) = r.quorem(&new_r);
            tup!((t, new_t) = (new_t.clone(), self.sub(&t, &(&q * &new_t))));
            tup!((r, new_r) = (new_r.clone(), rem));
        }

        if r > BigUint::from(1) {
            panic!("Not invertible");
        }

        t
    }

    /// Computes '(a / b) mod n'.
    /// Internally performs '(a * b^-1) mod n'
    pub fn div(&self, a: &BigUint, b: &BigUint) -> BigUint {
        self.mul(a, &self.inv(b))
    }

    /// Computes '-1*a mod n'
    pub fn negate(&self, a: &BigUint) -> BigUint {
        self.sub(self.n, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modulo_test() {
        let p = BigUint::from(7);
        let m = Modulo::new(&p);
        let x = m.inv(&2.into());
        assert_eq!(x, BigUint::from(4));
        assert_eq!(m.div(&1.into(), &2.into()), BigUint::from(4));
    }
}
