// This module contains all the core internal logic for the SecureBigUint.
//
// - None of these should be directly used outside of the parent module.
// - All these functions operate directly on [BaseType] slices without any
//   higher level templating.

use core::cmp::Ordering;

pub type BaseType = u32;
pub type SignedBaseType = i32;
pub const BASE_BITS: usize = 32;

// TODO: Make these private?
pub const BASE_BYTES: usize = core::mem::size_of::<BaseType>();
pub const BITS_PER_BYTE: usize = 8;

pub fn bit_width_impl(value: &[BaseType]) -> usize {
    value.len() * BASE_BITS
}

pub fn assign_zero(value: &mut [BaseType]) {
    for v in value.iter_mut() {
        *v = 0;
    }
}

pub fn bit_impl(value: &[BaseType], i: usize) -> usize {
    ((value[i / BASE_BITS] >> (i % BASE_BITS)) & 0b01) as usize
}

///////////////////////////////////////////////////////////////////////////////
/// Constant time utilities
///////////////////////////////////////////////////////////////////////////////

/// Copies 'self' to 'out' if should_copy is true. In all cases, this takes
/// a constant amount of time to execute.
///
/// NOTE: 'self' and 'out' must have the same bit_width().
#[inline(never)]
pub fn copy_if_impl(v: &[BaseType], should_copy: bool, out: &mut [BaseType]) {
    assert_eq!(v.len(), out.len());

    // Will be 0b111...111 if should_copy else 0.
    let self_mask = (!(should_copy as BaseType)).wrapping_add(1);

    let out_mask = !self_mask;

    for (self_v, out_v) in v.iter().zip(out.iter_mut()) {
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
pub fn swap_if_impl(lhs: &mut [BaseType], rhs: &mut [BaseType], should_swap: bool) {
    assert_eq!(lhs.len(), rhs.len());

    // Will be 0b111...111 if should_swap else 0.
    let mask = (!(should_swap as BaseType)).wrapping_add(1);

    for (self_v, other_v) in lhs.iter_mut().zip(rhs.iter_mut()) {
        // Will be 0 if we don't want to swap.
        let filter = mask & (*self_v ^ *other_v);

        *self_v ^= filter;
        *other_v ^= filter;
    }
}

///////////////////////////////////////////////////////////////////////////////
/// Bit Shifting Operations
///////////////////////////////////////////////////////////////////////////////

/// In-place reverses all the order of all bits in this integer.
pub fn reverse_bits_impl(value: &mut [BaseType]) {
    let mid = (value.len() + 1) / 2;
    for i in 0..mid {
        let j = value.len() - 1 - i;

        // Swap if we are not at the middle limb (only relevant if we have an odd number
        // of limbs).
        if i != j {
            value.swap(i, j);
            value[j] = value[j].reverse_bits();
        }

        value[i] = value[i].reverse_bits();
    }
}

#[must_use]
pub fn shl_impl(value: &mut [BaseType]) -> BaseType {
    let mut carry = 0;
    for v in value.iter_mut() {
        let (new_v, _) = v.overflowing_shl(1);
        let new_carry = *v >> 31;
        *v = new_v | carry;
        carry = new_carry;
    }

    carry
}

pub fn shr_impl(value: &mut [BaseType]) {
    let mut carry = 0;
    for v in value.iter_mut().rev() {
        let (new_v, _) = v.overflowing_shr(1);
        let new_carry = *v & 1;
        *v = new_v | (carry << 31);
        carry = new_carry;
    }
}

/// Computes 'self >>= n'
/// NOTE: We assume that 'n' is a publicly known constant.
pub fn shr_n_impl(value: &mut [BaseType], n: usize) {
    let byte_shift = n / BASE_BITS;
    let carry_size = n % BASE_BITS;
    let carry_mask = ((1 as BaseType) << carry_size).wrapping_sub(1);

    for i in 0..value.len() {
        let v = value[i];
        value[i] = 0;

        if i < byte_shift {
            continue;
        }

        let j = i - byte_shift;
        value[j] = v >> carry_size;

        if carry_size != 0 && j > 0 {
            let carry = v & carry_mask;
            value[j - 1] |= carry << (BASE_BITS - carry_size);
        }
    }
}

/// Computes self >>= BASE_BITS.
pub fn shr_base_impl(value: &mut [BaseType]) {
    assert_eq!(value[0], 0);
    for j in 1..value.len() {
        value[j - 1] = value[j];
    }
    let k = value.len();
    value[k - 1] = 0;
}

///////////////////////////////////////////////////////////////////////////////
/// Bitwise Operations
///////////////////////////////////////////////////////////////////////////////

pub fn and_assign_impl(lhs: &mut [BaseType], rhs: &[BaseType]) {
    for i in 0..lhs.len() {
        lhs[i] &= rhs[i];
    }
}

pub fn xor_assign_impl(lhs: &mut [BaseType], rhs: &[BaseType]) {
    assert_eq!(lhs.len(), rhs.len());

    for (lhs_value, rhs_value) in lhs.iter_mut().zip(rhs.iter()) {
        *lhs_value ^= *rhs_value;
    }
}

/// Performs 'lhs ^= rhs' only if 'should_apply' is true.
pub fn xor_assign_if_impl(lhs: &mut [BaseType], should_apply: bool, rhs: &[BaseType]) {
    assert!(lhs.len() >= rhs.len());

    // Will be 0b111...111 if should_apply else 0.
    let mask = (!(should_apply as BaseType)).wrapping_add(1);

    for i in 0..rhs.len() {
        lhs[i] ^= rhs[i] & mask;
    }
}

///////////////////////////////////////////////////////////////////////////////
/// Addition
///////////////////////////////////////////////////////////////////////////////

pub fn add_to_impl(lhs: &[BaseType], rhs: &[BaseType], output: &mut [BaseType]) {
    assert!(output.len() >= lhs.len());
    assert!(output.len() >= rhs.len());

    let mut carry = 0;
    // TODO: Always loop through max(lhs, rhs, output) length so we know for sure
    // that all carries are handled.
    let n = output.len();
    for i in 0..n {
        let a = lhs.get(i).cloned().unwrap_or(0);
        let b = rhs.get(i).cloned().unwrap_or(0);

        let v = (a as u64) + (b as u64) + carry;

        output[i] = v as BaseType;
        carry = v >> 32;
    }

    assert_eq!(carry, 0);
}

pub fn add_assign_impl(lhs: &mut [BaseType], rhs: &[BaseType]) {
    assert!(rhs.len() <= lhs.len());

    let mut carry = 0;
    let n = lhs.len();

    // TODO: Only loop up to rhs.len() + 1 if rhs is small.
    for i in 0..n {
        let v = (lhs[i] as u64) + (rhs.get(i).cloned().unwrap_or(0) as u64) + carry;

        lhs[i] = v as BaseType;
        carry = v >> 32;
    }

    assert_eq!(carry, 0);
}

///////////////////////////////////////////////////////////////////////////////
/// Subtraction
///////////////////////////////////////////////////////////////////////////////

/// TODO: Improve the constant time behavior of this.
/// It would be useful to have a conditional form of this that adds like
/// subtraction by zero.
pub fn overflowing_sub_assign_impl(lhs: &mut [BaseType], rhs: &[BaseType]) -> bool {
    let mut carry = 0;
    let n = lhs.len();
    for i in 0..n {
        // rhs is allowed to be narrower than self
        let r_i = if i < rhs.len() { rhs[i] } else { 0 };

        // TODO: Try to use overflowing_sub instead (that way we don't need to go to
        // 64bits)
        let v = (lhs[i] as i64) - (r_i as i64) + carry;
        if v < 0 {
            lhs[i] = (v + (u32::max_value() as i64) + 1) as u32;
            carry = -1;
        } else {
            lhs[i] = v as BaseType;
            carry = 0;
        }
    }

    carry != 0
}

pub fn overflowing_sub_to_impl(lhs: &[BaseType], rhs: &[BaseType], out: &mut [BaseType]) -> bool {
    let mut carry = 0;
    let n = out.len();
    for i in 0..n {
        let a = lhs.get(i).cloned().unwrap_or(0);
        let b = rhs.get(i).cloned().unwrap_or(0);

        // TODO: Try to use overflowing_sub instead (that way we don't need to go to
        // 64bits)
        let v = (a as i64) - (b as i64) + carry;
        if v < 0 {
            out[i] = (v + (u32::max_value() as i64) + 1) as u32;
            carry = -1;
        } else {
            out[i] = v as u32;
            carry = 0;
        }
    }

    carry != 0
}

///////////////////////////////////////////////////////////////////////////////
/// Multiplication
///////////////////////////////////////////////////////////////////////////////

/// O(n^2) multiplication. Assumes that u64*u64 multiplication is always
/// constant time.
///
/// 'out' must be twice the size of lhs/rhs
pub fn mul_to_impl(lhs: &[BaseType], rhs: &[BaseType], out: &mut [BaseType]) {
    // TODO: Deduplicate with add_mul_to_impl

    assign_zero(out);

    let mut overflowed = false;

    for i in 0..lhs.len() {
        let mut carry = 0;
        for j in 0..rhs.len() {
            // TODO: Ensure this uses the UMAAL instruction on ARM
            let tmp = ((lhs[i] as u64) * (rhs[j] as u64)) + (out[i + j] as u64) + carry;

            carry = tmp >> BASE_BITS;
            out[i + j] = tmp as BaseType;
        }

        // assert!(carry <= u32::max_value() as u64);
        if i + rhs.len() < out.len() {
            out[i + rhs.len()] = carry as BaseType;
        } else {
            overflowed |= carry != 0;
        }
    }

    assert!(!overflowed);
}

/// Multiplies two numbers and adds their result to the out number.
/// out += self*rhs
pub fn add_mul_to_impl(lhs: &[BaseType], rhs: &[BaseType], out: &mut [BaseType]) {
    let a = lhs;
    let b = rhs;

    for i in 0..a.len() {
        let mut carry = 0;
        for j in 0..b.len() {
            // TODO: Ensure this uses the UMAAL instruction on ARM
            let tmp = ((a[i] as u64) * (b[j] as u64)) + (out[i + j] as u64) + carry;

            carry = tmp >> BASE_BITS;
            out[i + j] = tmp as u32;
        }

        for k in (i + b.len())..out.len() {
            let tmp = (out[k] as u64) + carry;
            carry = tmp >> BASE_BITS;
            out[k] = tmp as u32;
        }
    }
}

///////////////////////////////////////////////////////////////////////////////
/// Carry-less Multiplication
///////////////////////////////////////////////////////////////////////////////

// TODO: Need a version of this using pmull in aarch64 (vmull_p64)

/// Interprates this integer and 'rhs' as polynomials over GF(2^n) and
/// multiplies them into 'out'.
///
/// Operations in this field:
/// - Addition is XOR
/// - Multiplication is AND
#[cfg(all(target_arch = "x86_64", target_feature = "pclmulqdq"))]
pub fn carryless_mul_to_impl(lhs: &[BaseType], rhs: &[BaseType], out: &mut [BaseType]) {
    use crate::intrinsics::*;
    use core::arch::x86_64::_mm_clmulepi64_si128;

    assert!(bit_width_impl(out) >= bit_width_impl(lhs) + bit_width_impl(rhs) - 1);

    assign_zero(out);

    for i in 0..lhs.len() {
        let a = u64_to_m128i(lhs[i] as u64);

        for j in 0..rhs.len() {
            let b = u64_to_m128i(rhs[j] as u64);

            let r = u64_from_m128i(unsafe { _mm_clmulepi64_si128(a, b, 0) });

            let rl = r as u32;
            let rh = (r >> 32) as u32;

            // Add to output
            out[i + j] ^= rl;
            out[i + j + 1] ^= rh;
        }
    }
}

#[cfg(not(all(target_arch = "x86_64", target_feature = "pclmulqdq")))]
pub fn carryless_mul_to_impl(lhs: &[BaseType], rhs: &[BaseType], out: &mut [BaseType]) {
    carryless_mul_to_generic(lhs, rhs, out)
}

fn carryless_mul_to_generic(lhs: &[BaseType], rhs: &[BaseType], out: &mut [BaseType]) {
    assert!(bit_width_impl(out) >= bit_width_impl(lhs) + bit_width_impl(rhs) - 1);
    assign_zero(out);
    for i in (0..bit_width_impl(rhs)).rev() {
        shl_impl(out);
        xor_assign_if_impl(out, bit_impl(rhs, i) == 1, &lhs);
    }
}

///////////////////////////////////////////////////////////////////////////////
/// Comparison
///////////////////////////////////////////////////////////////////////////////

pub fn cmp_impl(lhs: &[BaseType], rhs: &[BaseType]) -> Ordering {
    let mut less = 0;
    let mut greater = 0;

    let n = core::cmp::max(lhs.len(), rhs.len());
    for i in (0..n).rev() {
        let mask = !(less | greater);

        let a = lhs.get(i).cloned().unwrap_or(0);
        let b = rhs.get(i).cloned().unwrap_or(0);

        if a < b {
            less |= mask & 1;
        } else if a > b {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn carryless_mul_generic_test() {
        let a = &[0b10100010];
        let b = &[0b10010110];

        let mut out = [0u32; 2];

        carryless_mul_to_generic(&a[..], &b[..], &mut out);

        assert_eq!(&out[..], &[0b101100011101100, 0][..]);
    }
}
