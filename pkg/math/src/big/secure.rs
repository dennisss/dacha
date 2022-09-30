use alloc::vec::Vec;
use common::ceil_div;
use core::cmp::Ord;
use core::cmp::Ordering;
use core::marker::PhantomData;
use core::ops;
use core::ops::Div;
use core::ops::Index;
use core::ops::IndexMut;

use generic_array::{arr::AddLength, ArrayLength, GenericArray};
use typenum::Quot;
use typenum::{Prod, U32};

use crate::integer::Integer;
use crate::matrix::dimension::*;
use crate::number::{One, Zero};

/*
All SecureBigUints have a fixed_width() which describes exactly how many bits

bit_width()


Ideally have a generalized storage:
- Can either be:
    - Static size
    - Dynamic with max precision

Making a SecureBigUint requires specifying the precision.


    To create an element, we need to specify the precision (means we can't use zero() or one())


TODO: For from_le_bytes or from_be_bytes to be secure, they must be padded already to the max number of bytes.

*/

/*
pub trait StorageType<D: Dimension>:
    Clone + AsRef<[u32]> + AsMut<[u32]> + Index<usize, Output = u32> + IndexMut<usize, Output = u32>
{
    /// Allocates a new buffer with at least 'words' number of items.
    fn alloc(words: usize) -> Self;
}

impl StorageType<Dynamic> for Vec<u32> {
    fn alloc(words: usize) -> Self {
        vec![0; words]
    }
}
*/

/// Big unsigned integer implementation intended for security critical
/// use-cases.
///
/// Internally each instance stores a fixed size storage buffer based on the bit
/// width used to initialize the integer. All numerical operations are constant
/// time for a given storage buffer size unless otherwise specified.
#[derive(Clone)]
pub struct SecureBigUint {
    /// In little endian 32bits at a time.
    /// Will be padded with
    value: Vec<u32>,
}

impl SecureBigUint {
    fn from_usize(value: usize, width: usize) -> Self {
        let mut data = vec![0; ceil_div(width, 32)];
        data[0] = value as u32;

        Self { value: data }
    }
}

impl Integer for SecureBigUint {
    fn from_le_bytes(data: &[u8]) -> Self {
        // Necessary so that to_le_bytes() is lossless.
        assert!(data.len() % 4 == 0);

        let mut out = Self::from_usize(0, 8 * data.len());

        let n = data.len() / 4;
        for i in 0..(data.len() / 4) {
            out.value[i] = u32::from_le_bytes(*array_ref![data, 4 * i, 4]);
        }

        let rem = data.len() % 4;
        if rem != 0 {
            let mut rest = [0u8; 4];
            rest[0..rem].copy_from_slice(&data[(data.len() - rem)..]);

            out.value[n] = u32::from_le_bytes(rest);
        }

        out
    }

    fn to_le_bytes(&self) -> Vec<u8> {
        let mut data = vec![];
        data.reserve_exact(self.value.len() * 4);
        for v in &self.value {
            data.extend_from_slice(&v.to_le_bytes());
        }

        data
    }

    fn from_be_bytes(data: &[u8]) -> Self {
        todo!()
    }

    fn to_be_bytes(&self) -> Vec<u8> {
        todo!()
    }

    // TODO: Dedup with add_assign.
    fn add_to(&self, rhs: &Self, output: &mut Self) {
        assert_eq!(self.value.len(), rhs.value.len());

        let mut carry = 0;
        let n = self.value.len();
        for i in 0..n {
            let v = (self.value[i] as u64) + (rhs.value[i] as u64) + carry;

            output.value[i] = v as u32;
            carry = v >> 32;
        }

        assert_eq!(carry, 0);
    }

    fn add_assign(&mut self, rhs: &Self) {
        let mut carry = 0;
        let n = self.value.len();
        for i in 0..n {
            let v = (self.value[i] as u64) + (rhs.value[i] as u64) + carry;

            self.value[i] = v as u32;
            carry = v >> 32;
        }

        assert_eq!(carry, 0);
    }

    fn sub(&self, rhs: &Self) -> Self {
        let mut out = self.clone();
        out.sub_assign(rhs);
        out
    }

    /// TODO: Improve the constant time behavior of this.
    /// It would be useful to have a conditional form of this that adds like
    /// subtraction by zero.
    fn sub_assign(&mut self, rhs: &Self) {
        assert!(self.overflowing_sub_assign(rhs));
    }

    fn mul(&self, rhs: &Self) -> Self {
        let mut out = Self::from_usize(0, self.bit_width() + rhs.bit_width());
        self.mul_to(rhs, &mut out);
        out
    }

    /// O(n^2) multiplication. Assumes that u64*u64 multiplication is always
    /// constant time.
    ///
    /// 'out' must be twice the size of
    fn mul_to(&self, rhs: &Self, out: &mut Self) {
        assert_eq!(self.value.len(), rhs.value.len());

        let mid_idx = out.value.len() / 2;

        // All upper bytes must be zero so that we don't overflow the output container.
        // Multipling two numbers with 'n' bits will produce a result with '2*n' bits.
        for i in mid_idx..self.value.len() {
            assert_eq!(self.value[i], 0);
            assert_eq!(rhs.value[i], 0);
        }

        // Zero out the output.
        for i in 0..out.value.len() {
            out.value[i] = 0;
        }

        for i in 0..mid_idx {
            let mut carry = 0;
            for j in 0..mid_idx {
                let tmp = ((self.value[i] as u64) * (rhs.value[j] as u64))
                    + (out.value[i + j] as u64)
                    + carry;

                carry = tmp / ((u32::max_value() as u64) + 1); // '>> 32'
                out.value[i + j] = tmp as u32;
            }

            // assert!(carry <= u32::max_value() as u64);
            out.value[i + mid_idx] = carry as u32;
        }
    }

    fn bit(&self, i: usize) -> usize {
        ((self.value[i / 32] >> (i % 32)) & 0b01) as usize
    }

    fn set_bit(&mut self, i: usize, v: usize) {
        assert!(v == 0 || v == 1);
        let ii = i / 32;
        let shift = i % 32;
        let mask = !(1 << shift);

        self.value[ii] = (self.value[ii] & mask) | ((v as u32) << shift);
    }

    // Want to support large 2x sized number mod exact sized number.

    /*
    NOTE: quorem explicitly supports performing 'n % m' where 'n' has a larger bit_width than 'm' as this is useful for reducing the number of bits after multiplication.
    */

    fn quorem(&self, rhs: &Self) -> (Self, Self) {
        let mut q = Self::from_usize(0, self.bit_width()); // Range is [0, Self]
        let mut r = Self::from_usize(0, rhs.bit_width()); // Range is [0, rhs).

        // TODO: Instead use a conditional subtrack.
        // Then we could make this require just one output buffer and no internal
        // bffering.
        let zero = Self::from_usize(0, rhs.bit_width());

        // TODO: Implement a bit iterator so set_bit requires less work.
        for i in (0..self.bit_width()).rev() {
            let carry = r.shl();
            r.set_bit(0, self.bit(i));

            // TODO: If the RHS is public knowledge, then we should only need to do this
            // comparison once we reach the same number of bits as the RHS.
            // TODO: Make this '||' constant time
            let subtract = r >= *rhs || carry != 0;

            // TODO: Switch this to a wrapping_sub_assign as it will overflow if self is
            // larger than rhs
            r.overflowing_sub_assign(if subtract { rhs } else { &zero });
            q.set_bit(i, if subtract { 1 } else { 0 });
        }

        (q, r)
    }

    fn value_bits(&self) -> usize {
        todo!()
    }

    fn bit_width(&self) -> usize {
        self.value.len() * 32
    }
}

impl SecureBigUint {
    /*
    /// Computes 2^self more efficiently than using pow().
    /// Only supports exponents smaller than u32.
    /// TODO: Just take as input a u32 directly.
    pub fn exp2(&self) -> Self {
        let mut out = Self::zero();
        out.set_bit(self.value[0] as usize, 1);
        out
    }
    */

    // TODO: Having a checked_sub_to may be useful

    /// TODO: Improve the constant time behavior of this.
    /// It would be useful to have a conditional form of this that adds like
    /// subtraction by zero.
    fn overflowing_sub_assign(&mut self, rhs: &Self) -> bool {
        let mut carry = 0;
        let n = self.value.len();
        for i in 0..n {
            // TODO: Try to use overflowing_sub instead (that way we don't need to go to
            // 64bits)
            let v = (self.value[i] as i64) - (rhs.value[i] as i64) + carry;
            if v < 0 {
                self.value[i] = (v + (u32::max_value() as i64) + 1) as u32;
                carry = -1;
            } else {
                self.value[i] = v as u32;
                carry = 0;
            }
        }

        carry != 0
    }

    #[must_use]
    pub fn shl(&mut self) -> u32 {
        let mut carry = 0;
        for v in self.value.iter_mut() {
            let (new_v, _) = v.overflowing_shl(1);
            let new_carry = *v >> 31;
            *v = new_v | carry;
            carry = new_carry;
        }

        carry
    }

    pub fn shr(mut self, n: usize) -> Self {
        todo!()
    }

    pub fn and_assign(&mut self, rhs: &Self) {
        for i in 0..self.value.len() {
            self.value[i] &= rhs.value[i];
        }
    }
}

/*
Montgomery:
R: Always power of 2.

Must find R' using extended Euclidean algorithm
*/

impl Ord for SecureBigUint {
    fn cmp(&self, other: &Self) -> Ordering {
        assert_eq!(self.value.len(), other.value.len());

        let mut less = 0;
        let mut greater = 0;

        for i in (0..self.value.len()).rev() {
            let mask = !(less | greater);

            if self.value[i] < other.value[i] {
                less |= mask & 1;
            } else if self.value[i] > other.value[i] {
                greater |= mask & 1;
            }
        }

        let cmp = (less << 1) | greater;

        let mut out = Ordering::Equal;
        // Exactly one of these if statements should always be triggered.
        if cmp == 0b10 {
            out = Ordering::Less;
        }
        if cmp == 0b01 {
            out = Ordering::Greater;
        }
        if cmp == 0b00 {
            out = Ordering::Equal;
        }

        out
    }
}

impl PartialEq for SecureBigUint {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for SecureBigUint {}

impl PartialOrd for SecureBigUint {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::str::FromStr;

    #[test]
    fn secure_biguint_test() {
        // TODO: Check multiplication in x*0 and x*1 cases

        let seven = SecureBigUint::from_usize(7, 64);
        let one_hundred = SecureBigUint::from_usize(100, 64);

        assert!(one_hundred > seven);
        assert!(seven < one_hundred);
        assert!(one_hundred == one_hundred);
        assert!(seven == seven);

        let mut seven_hundred = SecureBigUint::from_usize(0, 64);
        seven.mul_to(&one_hundred, &mut seven_hundred);

        assert!(seven_hundred == SecureBigUint::from_usize(700, 64));

        let x = SecureBigUint::from_le_bytes(&[0xff, 0xff, 0xff, 0xff]);
        let mut temp = SecureBigUint::from_usize(0, 64);
        x.mul_to(&x, &mut temp);

        assert_eq!(
            &temp.to_le_bytes(),
            &(core::u32::MAX as u64).pow(2).to_le_bytes()
        );

        let (q, r) = temp.quorem(&x);

        // Equal to 'x' extended to 64 bits
        assert!(q == SecureBigUint::from_le_bytes(&[0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0]));

        assert!(r == SecureBigUint::from_usize(0, 32));

        let (q, r) = one_hundred.quorem(&seven);
        assert!(q == SecureBigUint::from_usize(14, 64));
        assert!(r == SecureBigUint::from_usize(2, 64));

        let (q, r) = seven.quorem(&one_hundred);
        assert!(q == SecureBigUint::from_usize(0, 64));
        assert!(r == SecureBigUint::from_usize(7, 64));

        // TODO: Test larger numbers.
    }
}
