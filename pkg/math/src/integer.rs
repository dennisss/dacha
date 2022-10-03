use alloc::vec::Vec;
use core::ops::{Add, Mul, Rem, Sub};

/// A number type containing positive or negative integers or zero.
///
/// Every instance of an Integer has some 'bit width' which defines how many
/// bits are reserved for storing the integer value including padding up to a
/// fixed width if applicable (e.g. u64's always have a fixed width of 64).
///
/// Keep in mind that operations such as clone/add/multiply retain the same safe
/// for the output buffer.
///
/// When performing security critical computations (crypto), the width of all
/// integers involved should be chosen as publicly known upper bounds of the
/// value sizes.
pub trait Integer: Clone + Ord + PartialOrd + Eq + PartialEq {
    // fn from_usize(value: usize, width: usize) -> Self;

    /// NOTE: The bit width is implied to be 8*data.len().
    fn from_le_bytes(data: &[u8]) -> Self;

    fn from_be_bytes(data: &[u8]) -> Self;

    fn to_le_bytes(&self) -> Vec<u8>;

    fn to_be_bytes(&self) -> Vec<u8>;

    /// Maximum number of bits of information this Integer instance can store.
    /// e.g. 64 for a u64 type integer
    ///
    /// This will be >= value_bits().
    fn bit_width(&self) -> usize;

    /// Minimum number of bits required to encode the current value of this
    /// integer.
    fn value_bits(&self) -> usize;

    fn bit(&self, i: usize) -> usize;

    fn set_bit(&mut self, i: usize, v: usize);

    fn add(&self, rhs: &Self) -> Self;

    fn add_into(mut self, rhs: &Self) -> Self {
        self.add_assign(rhs);
        self
    }

    fn add_to(&self, rhs: &Self, output: &mut Self);

    fn add_assign(&mut self, rhs: &Self);

    fn sub(&self, rhs: &Self) -> Self;
    // {
    //     self.clone().sub_into(rhs)
    // }

    fn sub_into(mut self, rhs: &Self) -> Self {
        self.sub_assign(rhs);
        self
    }

    fn sub_assign(&mut self, rhs: &Self);

    fn mul(&self, rhs: &Self) -> Self;
    //  {
    //     let mut out = Self::from_usize(0, self.bit_width());
    //     self.mul_to(rhs, &mut out);
    //     out
    // }

    fn mul_to(&self, rhs: &Self, out: &mut Self);

    fn quorem(&self, rhs: &Self) -> (Self, Self);

    fn rem(&self, rhs: &Self) -> Self {
        self.quorem(rhs).1
    }
}
