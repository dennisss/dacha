use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::ops::{
    Add, AddAssign, BitAndAssign, BitOrAssign, BitXorAssign, Div, DivAssign, Mul, Rem, RemAssign,
    Sub, SubAssign,
};

use common::ceil_div;
use common::errors::*;

use crate::integer::*;
use crate::number::*;

// 64bit max value:
// 9223372036854775807

#[derive(Clone)]
pub struct BigUint {
    // In little endian 32bits at a time.
    value: Vec<u32>,
}

impl Zero for BigUint {
    fn zero() -> Self {
        Self { value: vec![] }
    }

    fn is_zero(&self) -> bool {
        self.value.is_empty()
    }
}

impl One for BigUint {
    fn one() -> Self {
        Self { value: vec![1] }
    }

    fn is_one(&self) -> bool {
        self.value.len() == 1 && self.value[0] == 1
    }
}

impl Integer for BigUint {
    fn bit_width(&self) -> usize {
        self.value_bits()
    }

    fn value_bits(&self) -> usize {
        if self.value.len() == 0 {
            return 0;
        }

        const BASE_BITS: usize = 0u32.leading_zeros() as usize;
        let nz = self.value.last().unwrap().leading_zeros() as usize;
        if nz == BASE_BITS {
            panic!("Untrimmed big number");
        }

        BASE_BITS * (self.value.len() - 1) + (BASE_BITS - nz)
    }

    fn from_le_bytes(data: &[u8]) -> Self {
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

    fn from_be_bytes(data: &[u8]) -> Self {
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

    fn to_le_bytes(&self) -> Vec<u8> {
        // TODO: Don't go over the end of the nbits;
        let mut out = vec![];
        for v in self.value.iter() {
            let s = v.to_le_bytes();
            out.extend_from_slice(&s);
        }

        out
    }

    fn to_be_bytes(&self) -> Vec<u8> {
        // TODO: It is important not to add too many zeros.
        let mut out = vec![];
        for v in self.value.iter().rev() {
            let s = v.to_be_bytes();
            out.extend_from_slice(&s);
        }

        out
    }

    fn bit(&self, i: usize) -> usize {
        ((self.index(i / 32) >> (i % 32)) & 0b01) as usize
    }

    fn set_bit(&mut self, i: usize, v: usize) {
        assert!(v == 0 || v == 1);
        let ii = i / 32;
        let shift = i % 32;
        let mask = !(1 << shift);

        *self.index_mut(ii) = (self.index(ii) & mask) | ((v as u32) << shift);
        self.trim();
    }

    fn add(&self, rhs: &Self) -> Self {
        self.clone().add_into(rhs)
    }

    fn add_to(&self, rhs: &Self, output: &mut Self) {
        output.value.truncate(0);

        let mut carry = 0;
        let n = core::cmp::max(self.value.len(), rhs.value.len());
        for i in 0..n {
            let v = (self.index(i) as u64) + (rhs.index(i) as u64) + carry;

            *output.index_mut(i) = v as u32;
            carry = v >> 32;
        }

        if carry != 0 {
            output.value.push(carry as u32);
        }

        output.trim();
    }

    fn add_assign(&mut self, rhs: &Self) {
        let mut carry = 0;
        let n = core::cmp::max(self.value.len(), rhs.value.len());
        for i in 0..n {
            let v = (self.index(i) as u64) + (rhs.index(i) as u64) + carry;

            *self.index_mut(i) = v as u32;
            carry = v >> 32;
        }

        if carry != 0 {
            self.value.push(carry as u32);
        }

        self.trim();
    }

    fn sub(&self, rhs: &Self) -> Self {
        let mut v = self.clone();
        Integer::sub_assign(&mut v, rhs);
        v
    }

    fn sub_assign(&mut self, rhs: &Self) {
        let mut carry = 0;
        let n = core::cmp::max(self.value.len(), rhs.value.len());
        for i in 0..n {
            // TODO: Try to use overflowing_sub instead (that way we don't need to go to
            // 64bits)
            let v = (self.index(i) as i64) - (rhs.index(i) as i64) + carry;
            if v < 0 {
                *self.index_mut(i) = (v + (u32::max_value() as i64) + 1) as u32;
                carry = -1;
            } else {
                *self.index_mut(i) = v as u32;
                carry = 0;
            }
        }

        if carry != 0 {
            panic!("Subtraction less than zero");
        }

        self.trim();
    }

    fn mul(&self, rhs: &Self) -> Self {
        let mut out = Self::zero();
        self.mul_to(rhs, &mut out);
        out
    }

    fn mul_to(&self, rhs: &BigUint, out: &mut BigUint) {
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
    // BigUint) { 	let m = ceil_div(core::cmp::max(self.value.len(),
    // rhs.value.len()), 2); 	let x_1 = BigUint { value:  }
    // }

    /// Euclidean division returning a quotient and remainder
    ///
    /// Implemented as binary long division. See:
    /// https://en.wikipedia.org/wiki/Division_algorithm#Integer_division_(unsigned)_with_remainder
    fn quorem(&self, rhs: &BigUint) -> (Self, Self) {
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

        for i in (0..self.value_bits()).rev() {
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
}

impl BigUint {
    /// Returns the minimum number of bytes required to represent this number.
    pub fn min_bytes(&self) -> usize {
        ceil_div(self.value_bits(), 8)
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

    fn trim(&mut self) {
        while let Some(0) = self.value.last() {
            self.value.pop();
        }
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
        for i in 0..rhs.value_bits() {
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
                core::char::from_digit(r.value.first().cloned().unwrap_or(0), radix).unwrap(),
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

impl core::fmt::Display for BigUint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.to_string_radix(10))
    }
}

impl core::fmt::Debug for BigUint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(self, f)
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

#[cfg(target_pointer_width = "64")]
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

#[cfg(target_pointer_width = "32")]
impl From<usize> for BigUint {
    fn from(mut v: usize) -> Self {
        let mut out = Self::zero();
        out.value.push(v as u32);
        out
    }
}

// TODO: Implement generic radix form. (it should be especially efficient for
// power of 2 radixes)
impl core::str::FromStr for BigUint {
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

use core::ops;

impl_op_ex!(+= |lhs: &mut BigUint, rhs: &BigUint| {
    Integer::add_assign(lhs, rhs)
});

impl_op_commutative!(+ |lhs: BigUint, rhs: &BigUint| -> BigUint {
    Integer::add_into(lhs, rhs)
});

impl_op!(+ |lhs: &BigUint, rhs: &BigUint| -> BigUint {
    Integer::add(lhs, rhs)
});

impl_op_ex!(-= |lhs: &mut BigUint, rhs: &BigUint| {
    Integer::sub_assign(lhs, rhs)
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
            .resize(core::cmp::max(self.value.len(), rhs.value.len()), 0);
        for i in 0..self.value.len() {
            self.value[i] &= rhs.index(i);
        }
        self.trim();
    }
}

impl_op_ex!(&|lhs: &BigUint, rhs: &BigUint| -> BigUint {
    let mut out = BigUint::zero();
    out.value
        .resize(core::cmp::max(lhs.value.len(), rhs.value.len()), 0);
    for i in 0..out.value.len() {
        out.value[i] = lhs.index(i) & rhs.index(i);
    }
    out.trim();
    out
});

impl BitOrAssign<&BigUint> for BigUint {
    fn bitor_assign(&mut self, rhs: &BigUint) {
        self.value
            .resize(core::cmp::max(self.value.len(), rhs.value.len()), 0);
        for i in 0..self.value.len() {
            self.value[i] |= rhs.index(i);
        }
        self.trim();
    }
}

impl_op_ex!(^= |lhs: &mut BigUint, rhs: &BigUint| {
    lhs.value.resize(
        core::cmp::max(lhs.value.len(), rhs.value.len()), 0);
    for i in 0..lhs.value.len() {
        lhs.value[i] ^= rhs.index(i);
    }
    lhs.trim();
});

impl_op_ex!(^ |lhs: &BigUint, rhs: &BigUint| -> BigUint {
    let mut out = BigUint::zero();
    out.value.resize(
        core::cmp::max(lhs.value.len(), rhs.value.len()), 0);
    for i in 0..out.value.len() {
        out.value[i] = lhs.index(i) ^ rhs.index(i);
    }
    out.trim();
    out
});

impl_op_ex!(>> |a: &BigUint, shift: &BigUint| -> BigUint {
    let mut out = BigUint::zero();
    assert!(shift.value.len() <= 1);
    let s = shift.index(0) as usize;

    if s >= a.value_bits() {
        return out;
    }

    for i in 0..(a.value_bits() - s) {
        out.set_bit(i, a.bit(i + s));
    }

    out
});

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::string::ToString;
    use core::str::FromStr;

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
    fn biguint_shift() {
        let num = BigUint::from(0b10011100);
        let shift = BigUint::from(3);
        let expected = BigUint::from(0b10011);

        assert!((num >> shift) == expected);
    }
}
