use std::{marker::PhantomData, ops::Index};

use common::errors::*;
use protobuf::wire::{parse_varint, serialize_varint};

pub fn parse_slice(mut input: &[u8]) -> Result<(&[u8], &[u8])> {
    let len = parse_next!(input, parse_varint) as usize;
    if input.len() < len {
        return Err(err_msg("Slice out of range"));
    }

    Ok(input.split_at(len))
}

pub fn serialize_slice(data: &[u8], out: &mut Vec<u8>) {
    serialize_varint(data.len() as u64, out);
    out.extend_from_slice(data);
}

pub fn parse_string(mut input: &[u8]) -> Result<(String, &[u8])> {
    let data = parse_next!(input, parse_slice);
    Ok((String::from_utf8(data.to_vec())?, input))
}

pub fn serialize_string(value: &str, out: &mut Vec<u8>) {
    serialize_slice(value.as_bytes(), out);
}

pub fn parse_fixed32(input: &[u8]) -> Result<(u32, &[u8])> {
    if input.len() < 4 {
        return Err(err_msg("Input too short for fixed32"));
    }

    let val = u32::from_le_bytes(*array_ref![input, 0, 4]);
    Ok((val, &input[4..]))
}

pub fn parse_fixed64(input: &[u8]) -> Result<(u64, &[u8])> {
    if input.len() < 8 {
        return Err(err_msg("Input too short for fixed64"));
    }

    let val = u64::from_le_bytes(*array_ref![input, 0, 8]);
    Ok((val, &input[8..]))
}

pub fn parse_u8(input: &[u8]) -> Result<(u8, &[u8])> {
    if input.len() < 1 {
        return Err(err_msg("Input too short for u8"));
    }

    Ok((input[0], &input[1..]))
}

/*
DO NOT USE since this is unsafe unless we verify that memory is aligned.

// TODO: This assumes a native little-endian system.
// - we should swap the bytes in place if on a big-endian system
pub fn u32_slice(input: &[u8]) -> &[u32] {
    unsafe { std::slice::from_raw_parts(input.as_ptr() as *const u32, input.len() / 4) }
}
*/

use alloc::fmt::Debug;

#[derive(Clone, Copy)]
pub struct UnalignedSlice<'a, T: Copy> {
    data: *const T,
    len: usize,
    phantom: PhantomData<&'a ()>,
}

unsafe impl<'a, T: Sync + Send + Copy> Send for UnalignedSlice<'a, T> {}
unsafe impl<'a, T: Sync + Send + Copy> Sync for UnalignedSlice<'a, T> {}

impl<'a, T: Copy> UnalignedSlice<'a, T> {
    /// Only safe if T is a primitive type
    pub unsafe fn from_bytes(data: &'a [u8]) -> Self {
        assert!(data.len() % core::mem::size_of::<T>() == 0);
        let len = data.len() / core::mem::size_of::<T>();

        Self {
            data: core::mem::transmute(data.as_ptr()),
            len,
            phantom: PhantomData,
        }
    }

    pub fn get(&self, index: usize) -> T {
        assert!(index < self.len);
        unsafe { self.data.add(index).read_unaligned() }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn iter<'b>(&'b self) -> impl Iterator<Item = T> + 'b {
        UnalignedSliceIter {
            next_index: 0,
            inst: self,
        }
    }

    pub fn slice(&self, start: usize, end: usize) -> Self {
        assert!(end >= start);
        assert!(end <= self.len());
        assert!(start <= self.len());

        Self {
            data: unsafe { self.data.add(start) },
            len: end - start,
            phantom: self.phantom,
        }
    }
}

impl<'a, T: Copy + Debug> Debug for UnalignedSlice<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut out = String::new();
        out.push('[');
        for i in 0..self.len() {
            out.push_str(&format!("{:?}", self.get(i)));
            if i != self.len() - 1 {
                out.push_str(", ");
            }
        }
        out.push(']');

        write!(f, "{}", out)
    }
}

struct UnalignedSliceIter<'a, 'b, T: Copy> {
    next_index: usize,
    inst: &'b UnalignedSlice<'a, T>,
}

impl<'a, 'b, T: Copy> Iterator for UnalignedSliceIter<'a, 'b, T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.inst.len() {
            return None;
        }

        let idx = self.next_index;
        self.next_index += 1;
        Some(self.inst.get(idx))
    }
}
