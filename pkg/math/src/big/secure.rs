use std::cmp::Ord;
use std::cmp::Ordering;
use std::ops;
use std::ops::Div;

use generic_array::{arr::AddLength, ArrayLength, GenericArray};
use typenum::Quot;
use typenum::{Prod, U32};

/// Big unsigned integer implementation intended for security critical
/// use-cases.
///
/// NOTE: Some functions such as Debug/to_string() are naturally not implemented
/// securely.
///
/// TODO: Technically, I should use a CeilDiv for the types.
#[derive(Clone)]
pub struct SecureBigUint<Bits: ArrayLength<u32> + Div<U32>>
where
    Quot<Bits, U32>: ArrayLength<u32>,
{
    /// In little endian 32bits at a time.
    /// Will be padded with
    value: GenericArray<u32, Quot<Bits, U32>>,
}

impl<Bits: ArrayLength<u32> + Div<U32>> SecureBigUint<Bits>
where
    Quot<Bits, U32>: ArrayLength<u32>,
{
    pub fn zero() -> Self {
        Self {
            value: GenericArray::default(), // vec![0; max_num_bits / 32],
        }
    }

    // pub fn zero_like(&self) -> Self {
    //     Self::zero(self.value.len() * 32)
    // }

    pub fn from_le_bytes(data: &[u8]) -> Self {
        let mut out = Self::zero();

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

    pub fn to_le_bytes(&self) -> Vec<u8> {
        let mut data = vec![];
        data.reserve_exact(self.value.len() * 4);
        for v in &self.value {
            data.extend_from_slice(&v.to_le_bytes());
        }

        data
    }

    pub fn from(value: u32) -> Self {
        let mut num = Self::zero();
        num.value[0] = value;
        num
    }

    /// Computes 2^self more efficiently than using pow().
    /// Only supports exponents smaller than u32.
    /// TODO: Just take as input a u32 directly.
    pub fn exp2(&self) -> Self {
        let mut out = Self::zero();
        out.set_bit(self.value[0] as usize, 1);
        out
    }

    // pub fn add_to(&self, rhs: &Self, out: &mut Self) {
    //     assert_eq!(self.value.len(), rhs.value.len());
    //     assert_eq!(self.value.len(), out.value.len());

    //     for i in 0..self.value.len() {
    //         out.value[i] = self.value[i] + rhs.value[i];
    //     }
    // }

    // TODO: Having a checked_sub_to may be useful

    // pub fn add(&self, rhs: &Self) -> Self {
    //     let mut out = self.clone();
    //     out.add_assign(rhs);
    //     out
    // }

    pub fn add_assign(&mut self, rhs: &Self) {
        let mut carry = 0;
        let n = self.value.len();
        for i in 0..n {
            let v = (self.value[i] as u64) + (rhs.value[i] as u64) + carry;

            self.value[i] = v as u32;
            carry = v >> 32;
        }

        assert_eq!(carry, 0);
    }

    /// TODO: Improve the constant time behavior of this.
    /// It would be useful to have a conditional form of this that adds like
    /// subtraction by zero.
    pub fn sub_assign(&mut self, rhs: &Self) {
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

        assert_eq!(carry, 0);
    }

    pub fn mul(&self, rhs: &Self) -> Self {
        let mut out = Self::zero();
        self.mul_to(rhs, &mut out);
        out
    }

    /// O(n^2) multiplication. Assumes that u64*u64 multiplication is always
    /// constant time.
    pub fn mul_to(&self, rhs: &Self, out: &mut Self) {
        assert_eq!(self.value.len(), rhs.value.len());
        assert_eq!(self.value.len(), out.value.len());

        let mid_idx = self.value.len() / 2;

        // All upper bytes must be zero so that we don't overflow the container.
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

    pub fn max_num_bits(&self) -> usize {
        self.value.len() * 32
    }

    pub fn shl(&mut self) {
        let mut carry = 0;
        for v in self.value.iter_mut() {
            let (new_v, _) = v.overflowing_shl(1);
            let new_carry = *v >> 31;
            *v = new_v | carry;
            carry = new_carry;
        }
        assert_eq!(carry, 0);
    }

    pub fn shr(mut self, n: usize) -> Self {
        //
        self
    }

    pub fn bit(&self, i: usize) -> usize {
        ((self.value[i / 32] >> (i % 32)) & 0b01) as usize
    }

    pub fn set_bit(&mut self, i: usize, v: usize) {
        assert!(v == 0 || v == 1);
        let ii = i / 32;
        let shift = i % 32;
        let mask = !(1 << shift);

        self.value[ii] = (self.value[ii] & mask) | ((v as u32) << shift);
    }

    pub fn quorem(&self, rhs: &Self) -> (Self, Self) {
        let mut q = Self::zero();
        let mut r = Self::zero();
        let zero = Self::zero();

        for i in (0..self.max_num_bits()).rev() {
            r.shl();
            r.set_bit(0, self.bit(i));

            let subtract = r >= *rhs;

            r.sub_assign(if subtract { rhs } else { &zero });
            q.set_bit(i, if subtract { 1 } else { 0 });
        }

        (q, r)
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

impl<Bits: ArrayLength<u32> + Div<U32>> Ord for SecureBigUint<Bits>
where
    Quot<Bits, U32>: ArrayLength<u32>,
{
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

impl<Bits: ArrayLength<u32> + Div<U32>> PartialEq for SecureBigUint<Bits>
where
    Quot<Bits, U32>: ArrayLength<u32>,
{
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl<Bits: ArrayLength<u32> + Div<U32>> Eq for SecureBigUint<Bits> where
    Quot<Bits, U32>: ArrayLength<u32>
{
}

impl<Bits: ArrayLength<u32> + Div<U32>> PartialOrd for SecureBigUint<Bits>
where
    Quot<Bits, U32>: ArrayLength<u32>,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/*
impl<Bits: ArrayLength<u32> + Div<U32>> ops::AddAssign<&Self> for SecureBigUint<Bits>
where
    Quot<Bits, U32>: ArrayLength<u32>,
{
    fn add_assign(&mut self, rhs: &Self) {
        self.add_assign_impl(rhs);
    }
}

impl<Bits: ArrayLength<u32> + Div<U32>> ops::AddAssign<&Self> for SecureBigUint<Bits>
where
    Quot<Bits, U32>: ArrayLength<u32>,
{
*/

/*
impl_op_ex!(+= |lhs: &mut SecureBigUint, rhs: &SecureBigUint| {

});

impl_op_commutative!(+ |lhs: SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
    let mut out = lhs;
    out += rhs;
    out
});

impl_op!(+ |lhs: &SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
    // TODO: Optimize with a third buffer?
    lhs.clone() + rhs
});

impl_op_ex!(-= |lhs: &mut SecureBigUint, rhs: &SecureBigUint| {

});

impl_op_ex!(
    -|lhs: SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint {
        let mut out = lhs;
        out -= rhs;
        out
    }
);

impl_op!(-|lhs: &SecureBigUint, rhs: &SecureBigUint| -> SecureBigUint { lhs.clone() - rhs });
*/

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn secure_biguint_test() {
        // TODO: Check multiplication in x*0 and x*1 cases

        type Uint = SecureBigUint<typenum::U64>;

        let seven = Uint::from(7);
        let one_hundred = Uint::from(100);

        assert!(one_hundred > seven);
        assert!(seven < one_hundred);
        assert!(one_hundred == one_hundred);
        assert!(seven == seven);

        let mut seven_hundred = Uint::zero();
        seven.mul_to(&one_hundred, &mut seven_hundred);

        assert!(seven_hundred == Uint::from(700));

        let x = Uint::from_le_bytes(&[0xff, 0xff, 0xff, 0xff]);
        let mut temp = Uint::zero();
        x.mul_to(&x, &mut temp);

        assert_eq!(
            &temp.to_le_bytes(),
            &(std::u32::MAX as u64).pow(2).to_le_bytes()
        );

        let (q, r) = temp.quorem(&x);
        assert!(q == x);
        assert!(r == Uint::zero());

        let (q, r) = one_hundred.quorem(&seven);
        assert!(q == Uint::from(14));
        assert!(r == Uint::from(2));

        let (q, r) = seven.quorem(&one_hundred);
        assert!(q == Uint::from(0));
        assert!(r == Uint::from(7));

        // TODO: Test larger numbers.
    }
}
