use std::ops::{Index, IndexMut};

use common::fixed::vec::FixedVec;
use common::bits::{BitVector, BitVectorStorage};

// TODO: We can store all codes with just 8 bits + a 8-bit length specifier since the
// rest of the bits are always ones.
//
// Currently this takes up a usize + 2 bytes
pub type BitVector16 = BitVector<BitVectorStorage16>;


#[derive(Clone, Default)]
pub struct BitVectorStorage16 {
    data: [u8; 2]
}

impl Index<usize> for BitVectorStorage16 {
    type Output = u8;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.data[index]
    }
}

impl IndexMut<usize> for BitVectorStorage16 {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.data[index]
    }
}

impl AsRef<[u8]> for BitVectorStorage16 {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl AsMut<[u8]> for BitVectorStorage16 {
    #[inline]
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
}

impl<'a> From<&'a [u8]> for BitVectorStorage16 {
    fn from(slice: &'a [u8]) -> Self {
        todo!()
    }
}

impl BitVectorStorage for BitVectorStorage16 {
    #[inline(always)]
    fn clear(&mut self) {}

    #[inline(always)]
    fn resize(&mut self, _new_size: usize, _value: u8) {}

    #[inline(always)]
    fn push(&mut self, index: usize, value: u8) {
        self.data[index] = value;
    }

    #[inline(always)]
    fn len(&self) -> usize {
        2
    }
}