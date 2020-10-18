use common::ceil_div;
use common::errors::*;
use std::cmp::Ordering;
use std::ops::{
    Add, AddAssign, BitAndAssign, BitOrAssign, BitXorAssign, Div, DivAssign, Mul, Rem, RemAssign,
    Sub, SubAssign,
};

// 64bit max value:
// 9223372036854775807

#[derive(Clone)]
pub struct BigUint {
    // In little endian 32bits at a time.
    value: Vec<u32>,
}

impl BigUint {
    pub fn zero() -> Self {
        BigUint { value: vec![] }
    }

    pub fn is_zero(&self) -> bool {
        self.value.len() == 0
    }

    pub fn is_one(&self) -> bool {
        self.value.len() == 1 && self.value[0] == 1
    }

    pub fn from_le_bytes(data: &[u8]) -> Self {
        let mut value = vec![];
        value.reserve(ceil_div(data.len(), 4));

        for i in 0..(data.len() / 4) {
            value.push(u32::from_le_bytes(*array_ref![data, 4 * i, 4]));
        }

        let rem = data.len() % 4;
        if rem != 0 {
            let mut rest = [0u8; 4];
            rest[0..rem].copy_from_slice(&data[(data.len() - rem)..]);
            value.push(u32::from_le_bytes(rest))
        }

        let mut out = BigUint { value };
        out.trim();
        out
    }

    pub fn from_be_bytes(data: &[u8]) -> Self {
        let mut value = vec![];
        value.reserve(ceil_div(data.len(), 4));

        let rem = data.len() % 4;
        for i in (0..(data.len() / 4)).rev() {
            value.push(u32::from_be_bytes(*array_ref![data, rem + 4 * i, 4]));
        }

        if rem != 0 {
            let mut rest = [0u8; 4];
            rest[(4 - rem)..].copy_from_slice(&data[0..rem]);
            value.push(u32::from_be_bytes(rest))
        }

        let mut out = BigUint { value };
        // TODO: Trim is not crypto secure as it reveals the size of the numbers
        // as it stops early.
        out.trim();
        out
    }

    pub fn to_le_bytes(&self) -> Vec<u8> {
        // TODO: Don't go over the end of the nbits;
        let mut out = vec![];
        for v in self.value.iter() {
            let s = v.to_le_bytes();
            out.extend_from_slice(&s);
        }

        out
    }

    pub fn to_be_bytes(&self) -> Vec<u8> {
        // TODO: It is important not to add too many zeros.
        let mut out = vec![];
        for v in self.value.iter().rev() {
            let s = v.to_be_bytes();
            out.extend_from_slice(&s);
        }

        out
    }

    pub fn nbits(&self) -> usize {
        if self.value.len() == 0 {
            return 0;
        }

        const base_bits: usize = 0u32.leading_zeros() as usize;
        let nz = self.value.last().unwrap().leading_zeros() as usize;
        if nz == base_bits {
            panic!("Untrimmed big number");
        }

        base_bits * (self.value.len() - 1) + (base_bits - nz)
    }

    /// Returns the minimum number of bytes required to represent this number.
    pub fn min_bytes(&self) -> usize {
        ceil_div(self.nbits(), 8)
    }

    /// self << 1
    pub fn shl(&mut self) {
        let mut carry = 0;
        for v in self.value.iter_mut() {
            let (new_v, _) = v.overflowing_shl(1);
            let new_carry = *v >> 31;
            *v = new_v | carry;
            carry = new_carry;
        }
        if carry != 0 {
            self.value.push(carry);
        }
    }

    /// self >> 1
    pub fn shr(&mut self) {
        let mut carry = 0;
        for v in self.value.iter_mut() {
            let new_carry = *v & 0b1;
            *v = (*v >> 1) & (carry << 31);
            carry = new_carry;
        }

        self.trim();
    }

    fn index(&self, i: usize) -> u32 {
        self.value.get(i).cloned().unwrap_or(0)
    }

    fn index_mut(&mut self, i: usize) -> &mut u32 {
        if self.value.len() <= i {
            self.value.resize(i + 1, 0);
        }

        &mut self.value[i]
    }

    pub fn trim(&mut self) {
        while let Some(0) = self.value.last() {
            self.value.pop();
        }
    }

    /// TODO: Rename bit(i)
    pub fn bit(&self, i: usize) -> usize {
        ((self.index(i / 32) >> (i % 32)) & 0b01) as usize
    }

    pub fn set_bit(&mut self, i: usize, v: usize) {
        assert!(v == 0 || v == 1);
        let ii = i / 32;
        let shift = i % 32;
        let mask = !(1 << shift);

        *self.index_mut(ii) = (self.index(ii) & mask) | ((v as u32) << shift);
        self.trim();
    }

    pub fn mul_to(&self, rhs: &BigUint, out: &mut BigUint) {
        out.value.clear();
        out.value.reserve(self.value.len() + rhs.value.len());

        for i in 0..self.value.len() {
            let mut carry = 0;
            for j in 0..rhs.value.len() {
                let tmp = ((self.value[i] as u64) * (rhs.value[j] as u64))
                    + (out.index(i + j) as u64)
                    + carry;

                carry = tmp / ((u32::max_value() as u64) + 1);
                *out.index_mut(i + j) = tmp as u32;
            }

            // assert!(carry <= u32::max_value() as u64);
            *out.index_mut(i + rhs.value.len()) = carry as u32;
        }

        out.trim();
    }

    // TODO: We can avoid some temporaries by using references to split the BigUint
    // into two slices pub fn mul_karatsuba(&self, rhs: &BigUint, out: &mut
    // BigUint) { 	let m = ceil_div(std::cmp::max(self.value.len(),
    // rhs.value.len()), 2); 	let x_1 = BigUint { value:  }
    // }

    /// Euclidean division returning a quotient and remainder
    ///
    /// Implemented as binary long division. See:
    /// https://en.wikipedia.org/wiki/Division_algorithm#Integer_division_(unsigned)_with_remainder
    pub fn quorem(&self, rhs: &BigUint) -> (Self, Self) {
        // TODO: If the result is frequently 1, then it is cheaper to do a subtraction
        // first

        if rhs.is_zero() {
            panic!("Divide by zero");
        }
        if rhs.is_one() {
            return (self.clone(), BigUint::zero());
        }
        if self < rhs {
            return (BigUint::zero(), self.clone());
        }

        let mut q = BigUint::zero();
        let mut r = BigUint::zero();

        for i in (0..self.nbits()).rev() {
            r.shl();
            r.set_bit(0, self.bit(i));
            if r >= *rhs {
                r -= rhs;
                q.set_bit(i, 1);
            }
        }

        // TODO: Won't be needed if shl and set_bit both do this already.
        q.trim();
        r.trim();

        (q, r)
    }

    /// Computes self^rhs using t he repeated squaring algorithm.
    pub fn pow(&self, rhs: &BigUint) -> Self {
        if rhs.is_zero() {
            return BigUint::from(1);
        }
        if rhs.is_one() {
            return self.clone();
        }

        let mut out = Self::from(1);
        let mut p = self.clone();
        for i in 0..rhs.nbits() {
            if rhs.bit(i) == 1 {
                out = &out * &p;
            }
            p = &p * &p;
        }

        out
    }

    /// Computes 2^self more efficiently than using pow().
    /// Only supports exponents smaller than u32.
    pub fn exp2(&self) -> Self {
        assert_eq!(self.value.len(), 1);
        let mut out = Self::zero();
        out.set_bit(self.value[0] as usize, 1);
        out
    }

    /// TODO: Have a fast case for hex, etc..
    pub fn to_string_radix(&self, radix: u32) -> String {
        // TODO: These should be global constants (as well as one)
        let zero = BigUint::zero();
        let div = BigUint::from(radix as usize);

        let mut s = String::new();
        let mut tmp = self.clone();
        while tmp > zero {
            // TODO: We can divide by a larger power of 10 to make this more efficient.
            let (q, r) = tmp.quorem(&div);
            tmp = q;
            // TODO: Very inefficient
            s.insert(
                0,
                std::char::from_digit(r.value.first().cloned().unwrap_or(0), radix).unwrap(),
            );
        }

        if s.len() == 0 {
            s.push('0');
        }

        s
    }

    /// Computes the integer *floor* of the square root,
    /// NOTE: This does not check if the number is a perfect square.
    ///
    /// Uses the Newton method as described here:
    /// https://en.wikipedia.org/wiki/Integer_square_root#Using_only_integer_division
    pub fn isqrt(&self) -> BigUint {
        let mut x = self.clone();

        loop {
            let mut x_next = (self / &x) + &x;
            x_next.shr();

            // Check for convergence.
            if x_next == x {
                break;
            }

            // Check if the result increased by exactly one (means we have
            // started a cycle because 'self + 1' is a perfect square).
            x += BigUint::from(1);
            if x_next == x {
                break;
            }
        }

        x
    }
}

impl std::fmt::Display for BigUint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string_radix(10))
    }
}

impl std::fmt::Debug for BigUint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}

impl Ord for BigUint {
    /// TODO: This is not a constant time equality function and is not suitable
    /// for certain crypto circumstances.
    fn cmp(&self, other: &Self) -> Ordering {
        // NOTE: Assumes that there are no trailing zeros.
        if self.value.len() < other.value.len() {
            Ordering::Less
        } else if self.value.len() > other.value.len() {
            Ordering::Greater
        } else {
            for i in (0..self.value.len()).rev() {
                if self.value[i] < other.value[i] {
                    return Ordering::Less;
                } else if self.value[i] > other.value[i] {
                    return Ordering::Greater;
                }
            }

            Ordering::Equal
        }
    }
}

impl PartialEq for BigUint {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for BigUint {}

impl PartialOrd for BigUint {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<usize> for BigUint {
    fn from(mut v: usize) -> Self {
        let mut out = Self::zero();
        while v > 0 {
            out.value.push(v as u32);
            v = v >> 32;
        }
        out
    }
}

// TODO: Implement generic radix form. (it should be especially efficient for
// power of 2 radixes)
impl std::str::FromStr for BigUint {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        let ten = BigUint::from(10);

        let mut out = Self::zero();
        for c in s.chars() {
            let digit = c.to_digit(10).ok_or(err_msg("Invalid digit"))?;
            out = (&out * &ten) + &(digit as usize).into();
        }

        Ok(out)
    }
}

use std::ops;

impl_op_ex!(+= |lhs: &mut BigUint, rhs: &BigUint| {
    let mut carry = 0;
    let n = std::cmp::max(lhs.value.len(), rhs.value.len());
    for i in 0..n {
        let v = (lhs.index(i) as u64) + (rhs.index(i) as u64) + carry;

        *lhs.index_mut(i) = v as u32;
        carry = v >> 32;
    }

    if carry != 0 {
        lhs.value.push(carry as u32);
    }

    lhs.trim();
});

impl_op_commutative!(+ |lhs: BigUint, rhs: &BigUint| -> BigUint {
    let mut out = lhs;
    out += rhs;
    out
});

impl_op!(+ |lhs: &BigUint, rhs: &BigUint| -> BigUint {
    lhs.clone() + rhs
});

impl_op_ex!(-= |lhs: &mut BigUint, rhs: &BigUint| {
    let mut carry = 0;
    let n = std::cmp::max(lhs.value.len(), rhs.value.len());
    for i in 0..n {
        // TODO: Try to use overflowing_sub instead (that way we don't need to go to 64bits)
        let v = (lhs.index(i) as i64) - (rhs.index(i) as i64) + carry;
        if v < 0 {
            *lhs.index_mut(i) = (v + (u32::max_value() as i64) + 1) as u32;
            carry = -1;
        } else {
            *lhs.index_mut(i) = v as u32;
            carry = 0;
        }
    }

    if carry != 0 {
        panic!("Subtraction less than zero");
    }

    lhs.trim();
});

impl_op_ex!(-|lhs: BigUint, rhs: &BigUint| -> BigUint {
    let mut out = lhs;
    out -= rhs;
    out
});

impl_op!(-|lhs: &BigUint, rhs: &BigUint| -> BigUint { lhs.clone() - rhs });

impl_op_ex!(*|lhs: &BigUint, rhs: &BigUint| -> BigUint {
    let mut out = BigUint::zero();
    lhs.mul_to(rhs, &mut out);
    out
});

impl_op_ex!(/ |lhs: &BigUint, rhs: &BigUint| -> BigUint {
    let (q, _) = lhs.quorem(rhs);
    q
});

impl_op!(% |lhs: BigUint, rhs: &BigUint| -> BigUint {
    // TODO: This is redundant with the optimization in quorem.
    if &lhs < rhs {
        return lhs;
    }

    // Usually we find remainders frequently between each operation, so the numbers are still small and can be reduced just do a single subtraction.
    let mut out = lhs;
    out -= rhs;
    if &out < rhs {
        return out;
    }

    &out % rhs
});

impl_op!(% |lhs: &BigUint, rhs: &BigUint| -> BigUint {
    let (_, r) = lhs.quorem(rhs);
    r
});

impl BitAndAssign<&BigUint> for BigUint {
    fn bitand_assign(&mut self, rhs: &BigUint) {
        self.value
            .resize(std::cmp::max(self.value.len(), rhs.value.len()), 0);
        for i in 0..self.value.len() {
            self.value[i] &= rhs.index(i);
        }
        self.trim();
    }
}

impl_op_ex!(&|lhs: &BigUint, rhs: &BigUint| -> BigUint {
    let mut out = BigUint::zero();
    out.value
        .resize(std::cmp::max(lhs.value.len(), rhs.value.len()), 0);
    for i in 0..out.value.len() {
        out.value[i] = lhs.index(i) & rhs.index(i);
    }
    out.trim();
    out
});

impl BitOrAssign<&BigUint> for BigUint {
    fn bitor_assign(&mut self, rhs: &BigUint) {
        self.value
            .resize(std::cmp::max(self.value.len(), rhs.value.len()), 0);
        for i in 0..self.value.len() {
            self.value[i] |= rhs.index(i);
        }
        self.trim();
    }
}

impl_op_ex!(^= |lhs: &mut BigUint, rhs: &BigUint| {
    lhs.value.resize(
        std::cmp::max(lhs.value.len(), rhs.value.len()), 0);
    for i in 0..lhs.value.len() {
        lhs.value[i] ^= rhs.index(i);
    }
    lhs.trim();
});

impl_op_ex!(^ |lhs: &BigUint, rhs: &BigUint| -> BigUint {
    let mut out = BigUint::zero();
    out.value.resize(
        std::cmp::max(lhs.value.len(), rhs.value.len()), 0);
    for i in 0..out.value.len() {
        out.value[i] = lhs.index(i) ^ rhs.index(i);
    }
    out.trim();
    out
});

// NOTE: Currently not used.
impl_op_ex!(>> |a: &BigUint, shift: &BigUint| -> BigUint {
    let mut out = BigUint::zero();
    assert!(shift.value.len() <= 1);
    let s = shift.index(0) as usize;

    if s >= a.nbits() {
        return out;
    }

    for i in 0..(a.nbits() - s) {
        out.set_bit(i, a.bit(i + s));
    }

    out
});

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

        let mut out = BigUint::from(1);
        let mut p = a.clone();
        for i in 0..b.nbits() {
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
        let mut new_t = BigUint::from(1);
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
    use std::str::FromStr;

    #[test]
    fn biguint_test() {
        // TODO: Check multiplication in x*0 and x*1 cases

        let a = BigUint::from_str("10000000000000000000000000020").unwrap();
        let b = BigUint::from_str("1304").unwrap();
        assert_eq!((a * b).to_string(), "13040000000000000000000000026080");

        let c = BigUint::from_str("12345678912345678912345").unwrap();
        let d = BigUint::from_str("987654321987654321").unwrap();
        assert_eq!(
            (&c * &d).to_string(),
            "12193263135650053146912909516201119492745"
        );

        let e = BigUint::from_str("3").unwrap();
        let f = BigUint::from_str("7").unwrap();
        assert_eq!(e.pow(&f).to_string(), "2187");

        assert_eq!(BigUint::from(5) - BigUint::from(3), BigUint::from(2));

        assert_eq!(d.cmp(&c), Ordering::Less);
        assert_eq!(c.cmp(&d), Ordering::Greater);

        assert_eq!(d.quorem(&c).0.to_string(), "0");
        assert_eq!(d.quorem(&c).1.to_string(), "987654321987654321");

        assert_eq!(c.quorem(&d).0.to_string(), "12499");
        assert_eq!(c.quorem(&d).1.to_string(), "987541821987554166");

        assert_eq!(
            (c - d),
            BigUint::from_str("12344691258023691258024").unwrap()
        );
    }

    #[test]
    fn modulo_test() {
        let p = BigUint::from(7);
        let m = Modulo::new(&p);
        let x = m.inv(&2.into());
        assert_eq!(x, BigUint::from(4));
        assert_eq!(m.div(&1.into(), &2.into()), BigUint::from(4));
    }
}
