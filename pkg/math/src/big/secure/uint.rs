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
/// time for a given storage buffer size unless otherwise specified. This means
/// that we assume that the buffer widths are publicly known and don't vary with
/// the value of the integer.
///
/// Special care must be taken to ensure that the width of integers generated by
/// operations is kept under control:
/// - Addition (a + b) will output integers with space for one extra carry bit.
/// - Multiplication (a*b) will output integers with double the space.
/// - Operations like quorem (a % b) or truncate can be used to re-contrain the
///   width of integers.
#[derive(Clone, Debug)]
pub struct SecureBigUint {
    /// In little endian 32bits at a time.
    /// Will be padded with
    ///
    /// TODO: We can make this an enum to support passing in '&mut [BaseType]'
    pub(super) value: Vec<BaseType>,
}

impl SecureBigUint {
    /// Creates an integer from a small value that fits within a usize. The
    /// buffer used to store this number will be able to store at least 'width'
    /// bits.
    pub fn from_usize(value: usize, width: usize) -> Self {
        let mut data = vec![0; ceil_div(width, BASE_BITS)];
        data[0] = value as BaseType;

        Self { value: data }
    }
}

impl Integer for SecureBigUint {
    /// Creates an integer from little endian bytes representing the number.
    ///
    /// The width of the integer is inferred from data.len().
    /// The caller is responsible for ensuring that data.len() is a well known
    /// constant.
    fn from_le_bytes(data: &[u8]) -> Self {
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

    /// Converts the integer to little endian bytes.
    ///
    /// NOTE: This may have zero significant padding depending on the internal
    /// representation.
    fn to_le_bytes(&self) -> Vec<u8> {
        let mut data = vec![];
        data.reserve_exact(self.value.len() * BASE_BYTES);
        for v in &self.value {
            data.extend_from_slice(&v.to_le_bytes());
        }

        data
    }

    fn from_be_bytes(data: &[u8]) -> Self {
        let mut out = Self::from_usize(0, BITS_PER_BYTE * data.len());

        let n = data.len() / BASE_BYTES;
        for i in 0..(data.len() / BASE_BYTES) {
            out.value[i] = BaseType::from_be_bytes(*array_ref![
                data,
                data.len() - (BASE_BYTES * (i + 1)),
                BASE_BYTES
            ]);
        }

        let rem = data.len() % BASE_BYTES;
        if rem != 0 {
            let mut rest = [0u8; BASE_BYTES];
            rest[(BASE_BYTES - rem)..].copy_from_slice(&data[0..rem]);
            out.value[n] = BaseType::from_be_bytes(rest);
        }

        out
    }

    fn to_be_bytes(&self) -> Vec<u8> {
        let mut data = vec![];
        data.reserve_exact(self.value.len() * 4);
        for v in self.value.iter().rev() {
            data.extend_from_slice(&v.to_be_bytes());
        }

        data
    }

    /// Computes and returns 'self + rhs'. The output buffer will be 1 bit
    /// larger than the inputs to accomadate possible overflow.
    fn add(&self, rhs: &Self) -> Self {
        let mut out = Self::from_usize(0, core::cmp::max(self.bit_width(), rhs.bit_width()) + 1);
        self.add_to(rhs, &mut out);
        out
    }

    /// Computes 'output = self + rhs'. It is the user's responsibility to
    /// ensure that the
    fn add_to(&self, rhs: &Self, output: &mut Self) {
        assert!(output.value.len() >= self.value.len());
        assert!(output.value.len() >= rhs.value.len());

        let mut carry = 0;
        // TODO: Always loop through max(self, rhs, output) length so we know for sure
        // that all carries are handled.
        let n = output.value.len();
        for i in 0..n {
            let a = self.value.get(i).cloned().unwrap_or(0);
            let b = rhs.value.get(i).cloned().unwrap_or(0);

            let v = (a as u64) + (b as u64) + carry;

            output.value[i] = v as BaseType;
            carry = v >> 32;
        }

        assert_eq!(carry, 0);
    }

    /// Computes 'self += rhs'.
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

    /// It would be useful to have a conditional form of this that adds like
    /// subtraction by zero.
    fn sub_assign(&mut self, rhs: &Self) {
        assert!(!self.overflowing_sub_assign(rhs));
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
        out.assign_zero();

        let mut overflowed = false;

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
            if i + rhs.value.len() < out.value.len() {
                out.value[i + rhs.value.len()] = carry as BaseType;
            } else {
                overflowed |= carry != 0;
            }
        }

        assert!(!overflowed);
    }

    fn bit(&self, i: usize) -> usize {
        ((self.value[i / BASE_BITS] >> (i % BASE_BITS)) & 0b01) as usize
    }

    fn set_bit(&mut self, i: usize, v: usize) {
        assert!(v == 0 || v == 1);
        let ii = i / BASE_BITS;
        let shift = i % BASE_BITS;
        let mask = !(1 << shift);

        self.value[ii] = (self.value[ii] & mask) | ((v as BaseType) << shift);
    }

    /// Computes the quotient and remainder of 'self / rhs'.
    ///
    /// Any mixture of input bit_widths is supported.
    /// Internally this uses binary long division.
    ///
    /// NOTE: This is very slow and should be avoided if possible.
    ///
    /// Returns a tuple of '(self / rhs, self % rhs)' where the quotient is the
    /// same width as 'self' and the remainder is the same width as 'rhs'.
    fn quorem(&self, rhs: &Self) -> (Self, Self) {
        let mut q = Self::from_usize(0, self.bit_width()); // Range is [0, Self]
        let mut r = Self::from_usize(0, rhs.bit_width()); // Range is [0, rhs).

        // TODO: Implement a bit iterator so set_bit requires less work.
        for i in (0..self.bit_width()).rev() {
            let carry = r.shl();
            r.set_bit(0, self.bit(i));

            let mut next_r = Self::from_usize(0, rhs.bit_width());

            // If there is a carry, then we know that r might be > rhs when the shl also has
            // a carry.
            let carry2 = r.overflowing_sub_to(rhs, &mut next_r);

            let subtract = (carry != 0) == carry2;

            next_r.copy_if(subtract, &mut r);

            q.set_bit(i, if subtract { 1 } else { 0 });
        }

        (q, r)
    }

    fn value_bits(&self) -> usize {
        for i in (0..self.value.len()).rev() {
            let zeros = self.value[i].leading_zeros() as usize;
            if zeros == BASE_BITS {
                continue;
            }

            return (i * BASE_BITS) + (BASE_BITS - zeros);
        }

        0
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
    #[inline(never)]
    pub fn copy_if(&self, should_copy: bool, out: &mut Self) {
        assert_eq!(self.value.len(), out.value.len());

        // Will be 0b111...111 if should_copy else 0.
        let self_mask = (!(should_copy as BaseType)).wrapping_add(1);

        let out_mask = !self_mask;

        for (self_v, out_v) in self.value.iter().zip(out.value.iter_mut()) {
            *out_v = (*self_v & self_mask).wrapping_add(*out_v & out_mask);
        }
    }

    /// Swaps the contents of 'self' and 'other' if 'should_swap' is true.
    ///
    /// The actual values of both integers are swapped rather than swapping any
    /// internal memory pointers so that 'should_swap' can not be inferred from
    /// the memory locations of the final integers.
    ///
    /// At a given integer bit_width, this should always take the same amount of
    /// CPU cycles to execute.
    #[inline(never)]
    pub fn swap_if(&mut self, other: &mut Self, should_swap: bool) {
        assert_eq!(self.value.len(), other.value.len());

        // Will be 0b111...111 if should_swap else 0.
        let mask = (!(should_swap as BaseType)).wrapping_add(1);

        for (self_v, other_v) in self.value.iter_mut().zip(other.value.iter_mut()) {
            // Will be 0 if we don't want to swap.
            let filter = mask & (*self_v ^ *other_v);

            *self_v ^= filter;
            *other_v ^= filter;
        }
    }

    /// In-place reverses all the order of all bits in this integer.
    pub fn reverse_bits(&mut self) {
        let mid = (self.value.len() + 1) / 2;
        for i in 0..mid {
            let j = self.value.len() - 1 - i;

            // Swap if we are not at the middle limb (only relevant if we have an odd number
            // of limbs).
            if i != j {
                self.value.swap(i, j);
                self.value[j] = self.value[j].reverse_bits();
            }

            self.value[i] = self.value[i].reverse_bits();
        }
    }

    /// Performs 'self ^= rhs' only if 'should_apply' is true.
    pub fn xor_assign_if(&mut self, should_apply: bool, rhs: &Self) {
        assert_eq!(self.value.len(), rhs.value.len());

        // Will be 0b111...111 if should_apply else 0.
        let mask = (!(should_apply as BaseType)).wrapping_add(1);

        for i in 0..self.value.len() {
            self.value[i] ^= rhs.value[i] & mask;
        }
    }

    pub fn discard(&mut self, bit_width: usize) {
        let n = ceil_div(bit_width, BASE_BITS);
        self.value.truncate(n);
    }

    ///
    pub fn truncate(&mut self, bit_width: usize) {
        let n = ceil_div(bit_width, BASE_BITS);

        // TODO: Also zero out any high bits

        for i in n..self.value.len() {
            assert_eq!(self.value[i], 0);
        }
        self.value.truncate(n);
    }

    /// Computes 2^n more efficiently than using pow().
    /// Only supports exponents smaller than u32.
    /// TODO: Just take as input a u32 directly.
    pub fn exp2(n: u32, bit_width: usize) -> Self {
        let mut out = Self::from_usize(0, bit_width);
        out.set_bit(n as usize, 1);
        out
    }

    pub fn is_zero(&self) -> bool {
        let mut is = true;

        for v in &self.value {
            is &= *v == 0;
        }

        is
    }

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
        let n = out.value.len();
        for i in 0..n {
            let a = self.value.get(i).cloned().unwrap_or(0);
            let b = rhs.value.get(i).cloned().unwrap_or(0);

            // TODO: Try to use overflowing_sub instead (that way we don't need to go to
            // 64bits)
            let v = (a as i64) - (b as i64) + carry;
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

    /// Performs modular reduction using up to one subtraction of the modulus
    /// from the value.
    ///
    /// Will panic if 'self' was >= 2*modulus
    pub(super) fn reduce_once(&mut self, modulus: &Self) {
        let mut reduced = Self::from_usize(0, self.bit_width());
        let overflow = self.overflowing_sub_to(modulus, &mut reduced);
        reduced.copy_if(!overflow, self);
        self.truncate(modulus.bit_width());
    }

    #[must_use]
    pub fn shl(&mut self) -> BaseType {
        let mut carry = 0;
        for v in self.value.iter_mut() {
            let (new_v, _) = v.overflowing_shl(1);
            let new_carry = *v >> 31;
            *v = new_v | carry;
            carry = new_carry;
        }

        carry
    }

    pub fn shr(&mut self) {
        let mut carry = 0;
        for v in self.value.iter_mut().rev() {
            let (new_v, _) = v.overflowing_shr(1);
            let new_carry = *v & 1;
            *v = new_v | (carry << 31);
            carry = new_carry;
        }
    }

    /// Computes self >>= BASE_BITS.
    pub(super) fn shr_base(&mut self) {
        assert_eq!(self.value[0], 0);
        for j in 1..self.value.len() {
            self.value[j - 1] = self.value[j];
        }
        let k = self.value.len();
        self.value[k - 1] = 0;
    }

    pub fn and_assign(&mut self, rhs: &Self) {
        for i in 0..self.value.len() {
            self.value[i] &= rhs.value[i];
        }
    }

    // TODO: Need a version of this using pmull in aarch64 (vmull_p64)

    /// Interprates this integer and 'rhs' as polynomials over GF(2^n) and
    /// multiplies them into 'out'.
    ///
    /// Operations in this field:
    /// - Addition is XOR
    /// - Multiplication is AND
    #[cfg(all(target_arch = "x86_64", target_feature = "pclmulqdq"))]
    pub fn carryless_mul_to(&self, rhs: &Self, out: &mut Self) {
        use crate::intrinsics::*;
        use core::arch::x86_64::_mm_clmulepi64_si128;

        assert!(out.bit_width() >= self.bit_width() + rhs.bit_width() - 1);

        out.assign_zero();

        for i in 0..self.value.len() {
            let a = u64_to_m128i(self.value[i] as u64);

            for j in 0..rhs.value.len() {
                let b = u64_to_m128i(rhs.value[j] as u64);

                let r = u64_from_m128i(unsafe { _mm_clmulepi64_si128(a, b, 0) });

                let rl = r as u32;
                let rh = (r >> 32) as u32;

                // Add to output
                out.value[i + j] ^= rl;
                out.value[i + j + 1] ^= rh;
            }
        }
    }

    // TODO: Finish making this constant time and correct.
    #[cfg(not(all(target_arch = "x86_64", target_feature = "pclmulqdq")))]
    pub fn carryless_mul_to(&self, rhs: &Self, out: &mut Self) {
        assert!(out.bit_width() >= self.bit_width() + rhs.bit_width() - 1);

        out.assign_zero();
        for i in 0..b.value_bits() {
            out.xor_assign_if(b.bit(i) == 1, &a);
            a.shl();
        }
    }

    // TODO: Move to a shared utility.
    pub fn to_string_radix(&self, radix: u32) -> alloc::string::String {
        // TODO: These should be global constants (as well as one)
        let zero = Self::from_usize(0, self.bit_width());
        let div = Self::from_usize(radix as usize, 32);

        let mut s = alloc::string::String::new();
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

    /// Resets the value of the integer to 0.
    pub fn assign_zero(&mut self) {
        for v in self.value.iter_mut() {
            *v = 0;
        }
    }

    /// In-place increases the size
    pub fn extend(&mut self, bit_width: usize) {
        let new_len = ceil_div(bit_width, BASE_BITS);
        assert!(new_len >= self.value.len());
        self.value.resize(new_len, 0);
    }

    pub fn from_str(s: &str, bit_width: usize) -> common::errors::Result<Self> {
        let ten = SecureBigUint::from_usize(10, 32);

        let mut out = Self::from_usize(0, bit_width);
        for c in s.chars() {
            let digit = c
                .to_digit(10)
                .ok_or(common::errors::err_msg("Invalid digit"))?;

            let tmp = out.clone();
            ten.mul_to(&tmp, &mut out);

            out += SecureBigUint::from_usize(digit as usize, bit_width);

            // out = (&out * &ten) + &(digit as usize).into();
        }

        Ok(out)
    }
}

impl core::fmt::Display for SecureBigUint {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.to_string_radix(10))
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
    // NOTE: Does not use add_into to avoid risking an overflow.
    Integer::add(&lhs, rhs)
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

impl_op_ex!(^= |lhs: &mut SecureBigUint, rhs: &SecureBigUint| {
    assert_eq!(lhs.value.len(), rhs.value.len());

    for (lhs_value, rhs_value) in lhs.value.iter_mut().zip(rhs.value.iter()) {
        *lhs_value ^= *rhs_value;
    }
});

impl_op_ex!(^ |lhs: &SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
    assert_eq!(lhs.value.len(), rhs.value.len());

    let mut out = SecureBigUint::from_usize(0, lhs.bit_width());

    for i in 0..out.value.len() {
        out.value[i] = lhs.value[i] ^ rhs.value[i];
    }

    out
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