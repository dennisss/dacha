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
The security of this stuff depends on 32bit x 32bit -> 64bit multiplication to

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

pub(super) type BaseType = u32;
pub(super) const BASE_BITS: usize = 32;

const BASE_BYTES: usize = core::mem::size_of::<BaseType>();
const BITS_PER_BYTE: usize = 8;

/// Big unsigned integer implementation intended for security critical
/// use-cases.
///
/// Internally each instance stores a fixed size storage buffer based on the bit
/// width used to initialize the integer. All numerical operations are constant
/// time for a given storage buffer size unless otherwise specified.
#[derive(Clone, Debug)]
pub struct SecureBigUint {
    /// In little endian 32bits at a time.
    /// Will be padded with
    ///
    /// TODO: We can make this an enum to support passing in '&mut [BaseType]'
    pub(super) value: Vec<BaseType>,
}

impl SecureBigUint {
    pub fn from_usize(value: usize, width: usize) -> Self {
        let mut data = vec![0; ceil_div(width, BASE_BITS)];
        data[0] = value as u32;

        Self { value: data }
    }
}

impl Integer for SecureBigUint {
    fn from_le_bytes(data: &[u8]) -> Self {
        // Necessary so that to_le_bytes() is lossless.
        assert!(data.len() % BASE_BYTES == 0);

        let mut out = Self::from_usize(0, BITS_PER_BYTE * data.len());

        let n = data.len() / BASE_BYTES;
        for i in 0..(data.len() / BASE_BYTES) {
            out.value[i] = BaseType::from_le_bytes(*array_ref![data, BASE_BYTES * i, BASE_BYTES]);
        }

        let rem = data.len() % BASE_BYTES;
        if rem != 0 {
            let mut rest = [0u8; BASE_BYTES];
            rest[0..rem].copy_from_slice(&data[(data.len() - rem)..]);
            out.value[n] = BaseType::from_le_bytes(rest);
        }

        out
    }

    fn to_le_bytes(&self) -> Vec<u8> {
        let mut data = vec![];
        data.reserve_exact(self.value.len() * BASE_BYTES);
        for v in &self.value {
            data.extend_from_slice(&v.to_le_bytes());
        }

        data
    }

    fn from_be_bytes(data: &[u8]) -> Self {
        // Necessary so that to_be_bytes() is lossless.
        assert!(data.len() % BASE_BYTES == 0);

        let mut value = vec![];
        value.reserve_exact(data.len() / 4);

        for chunk in data.chunks(4).rev() {
            value.push(BaseType::from_be_bytes(*array_ref![chunk, 0, 4]));
        }

        Self { value }
    }

    fn to_be_bytes(&self) -> Vec<u8> {
        let mut data = vec![];
        data.reserve_exact(self.value.len() * 4);
        for v in self.value.iter().rev() {
            data.extend_from_slice(&v.to_be_bytes());
        }

        data
    }

    // TODO: Dedup with add_assign.
    fn add_to(&self, rhs: &Self, output: &mut Self) {
        assert_eq!(self.value.len(), rhs.value.len());

        let mut carry = 0;
        let n = self.value.len();
        for i in 0..n {
            let v = (self.value[i] as u64) + (rhs.value[i] as u64) + carry;

            output.value[i] = v as BaseType;
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
        // Zero out the output.
        for i in 0..out.value.len() {
            out.value[i] = 0;
        }

        for i in 0..self.value.len() {
            let mut carry = 0;
            for j in 0..rhs.value.len() {
                // TODO: Ensure this uses the UMAAL instruction on ARM
                let tmp = ((self.value[i] as u64) * (rhs.value[j] as u64))
                    + (out.value[i + j] as u64)
                    + carry;

                carry = tmp >> BASE_BITS;
                out.value[i + j] = tmp as BaseType;
            }

            // assert!(carry <= u32::max_value() as u64);
            out.value[i + rhs.value.len()] = carry as BaseType;
        }
    }

    fn bit(&self, i: usize) -> usize {
        ((self.value[i / BASE_BITS] >> (i % BASE_BITS)) & 0b01) as usize
    }

    fn set_bit(&mut self, i: usize, v: usize) {
        assert!(v == 0 || v == 1);
        let ii = i / BASE_BITS;
        let shift = i % BASE_BITS;
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

            let mut next_r = Self::from_usize(0, rhs.bit_width());

            // If there is a carry, then we know that r might be > rhs when the shl also has
            // a carry.
            let carry2 = r.overflowing_sub_to(rhs, &mut next_r);

            let subtract = (carry != 0) == carry2;

            // TODO: If the RHS is public knowledge, then we should only need to do this
            // comparison once we reach the same number of bits as the RHS.
            // TODO: Make this '||' constant time
            // let subtract = r >= *rhs || carry != 0;

            // TODO: Switch this to a wrapping_sub_assign as it will overflow if self is
            // larger than rhs
            // TODO: Can perform comparison by always subtracking.
            // r.overflowing_sub_assign(if subtract { rhs } else { &zero });

            if subtract {
                r = next_r;
            }

            q.set_bit(i, if subtract { 1 } else { 0 });
        }

        (q, r)
    }

    fn value_bits(&self) -> usize {
        todo!()
    }

    fn bit_width(&self) -> usize {
        self.value.len() * BASE_BITS
    }
}

impl SecureBigUint {
    /// Multiplies two numbers and adds their result to the out number.
    /// out += self*rhs
    pub(super) fn add_mul_to(&self, rhs: &Self, out: &mut Self) {
        let a = &self.value[..];
        let b = &rhs.value[..];

        for i in 0..a.len() {
            let mut carry = 0;
            for j in 0..b.len() {
                // TODO: Ensure this uses the UMAAL instruction on ARM
                let tmp = ((a[i] as u64) * (b[j] as u64)) + (out.value[i + j] as u64) + carry;

                carry = tmp >> BASE_BITS;
                out.value[i + j] = tmp as u32;
            }

            for k in (i + b.len())..out.value.len() {
                let tmp = (out.value[k] as u64) + carry;
                carry = tmp >> BASE_BITS;
                out.value[k] = tmp as u32;
            }
        }
    }

    /// Copies 'self' to 'out' if should_copy is true. In all cases, this takes
    /// a constant amount of time to execute.
    ///
    /// NOTE: 'self' and 'out' must have the same bit_width().
    pub(super) fn copy_if(&self, should_copy: bool, out: &mut Self) {
        // Will be 0b111...111 if should_copy else 0.
        let self_mask = (!(should_copy as BaseType)).wrapping_add(1);

        let out_mask = !self_mask;

        for (self_v, out_v) in self.value.iter().zip(out.value.iter_mut()) {
            *out_v = (*self_v & self_mask).wrapping_add(*out_v & out_mask);
        }
    }

    ///
    pub fn truncate(&mut self, bit_width: usize) {
        let n = ceil_div(bit_width, BASE_BITS);

        for i in n..self.value.len() {
            assert_eq!(self.value[i], 0);
        }
        self.value.truncate(n);
    }

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

    pub fn is_zero(&self) -> bool {
        let mut is = true;

        for v in &self.value {
            is &= *v == 0;
        }

        is
    }

    // TODO: Having a checked_sub_to may be useful

    /// TODO: Improve the constant time behavior of this.
    /// It would be useful to have a conditional form of this that adds like
    /// subtraction by zero.
    pub(super) fn overflowing_sub_assign(&mut self, rhs: &Self) -> bool {
        let mut carry = 0;
        let n = self.value.len();
        for i in 0..n {
            // rhs is allowed to be narrower than self
            let r_i = if i < rhs.value.len() { rhs.value[i] } else { 0 };

            // TODO: Try to use overflowing_sub instead (that way we don't need to go to
            // 64bits)
            let v = (self.value[i] as i64) - (r_i as i64) + carry;
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

    pub(super) fn overflowing_sub_to(&self, rhs: &Self, out: &mut Self) -> bool {
        let mut carry = 0;
        let n = self.value.len();
        for i in 0..n {
            // TODO: Try to use overflowing_sub instead (that way we don't need to go to
            // 64bits)
            let v = (self.value[i] as i64) - (rhs.value[i] as i64) + carry;
            if v < 0 {
                out.value[i] = (v + (u32::max_value() as i64) + 1) as u32;
                carry = -1;
            } else {
                out.value[i] = v as u32;
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

impl_op_ex!(+= |lhs: &mut SecureBigUint, rhs: &SecureBigUint| {
    Integer::add_assign(lhs, rhs)
});

impl_op_commutative!(+ |lhs: SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
    Integer::add_into(lhs, rhs)
});

impl_op!(+ |lhs: &SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
    Integer::add(lhs, rhs)
});

impl_op_ex!(-= |lhs: &mut SecureBigUint, rhs: &SecureBigUint| {
    Integer::sub_assign(lhs, rhs)
});

impl_op_ex!(-|lhs: SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint { lhs.sub_into(rhs) });

impl_op!(-|lhs: &SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint { Integer::sub(lhs, rhs) });

impl_op_ex!(
    *|lhs: &SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint { Integer::mul(lhs, rhs) }
);

impl_op_ex!(/ |lhs: &SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
    let (q, _) = lhs.quorem(rhs);
    q
});

impl_op!(% |lhs: SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
    let (_, r) = lhs.quorem(rhs);
    r
});

impl_op!(% |lhs: &SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
    let (_, r) = lhs.quorem(rhs);
    r
});

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
