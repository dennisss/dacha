use super::BigUint;
use common::errors::*;
use common::vec::VecPtr;

/// Signed arbitrary length integer.
#[derive(Clone, PartialEq)]
pub struct BigInt {
    // In little endian 32bits at a time.
    value: VecPtr<u32>,
}

impl BigInt {
    pub fn zero() -> Self {
        Self {
            value: VecPtr::from_vec(vec![]),
        }
    }

    pub const fn from_le_static(data: &'static [u32]) -> Self {
        // TODO: THe main issue is that this doesn't gurantee that it is trimmed.
        Self {
            value: VecPtr::from_static(data),
        }
    }

    pub fn from_le_bytes(data: &[u8]) -> Self {
        let mut parts = vec![];
        for i in 0..(data.len() / 4) {
            parts.push(u32::from_le_bytes(*array_ref![data, 4 * i, 4]));
        }

        let r = data.len() % 4;
        if r != 0 {
            let mut buf = if data[data.len() - 1] & 0x80 != 0 {
                [0xffu8; 4]
            } else {
                [0u8; 4]
            };

            buf[0..r].copy_from_slice(&data[(data.len() - r)..]);
            parts.push(u32::from_le_bytes(buf));
        }

        let mut out = Self {
            value: VecPtr::from_vec(parts),
        };
        out.trim();
        out
    }

    pub fn from_be_bytes(data: &[u8]) -> Self {
        let mut parts = vec![];
        let r = data.len() % 4;
        for i in (0..(data.len() / 4)).rev() {
            parts.push(u32::from_be_bytes(*array_ref![data, r + 4 * i, 4]));
        }

        if r != 0 {
            let mut buf = if data[0] & 0x80 != 0 {
                [0xffu8; 4]
            } else {
                [0u8; 4]
            };

            buf[(4 - r)..].copy_from_slice(&data[0..r]);
            parts.push(u32::from_be_bytes(buf));
        }

        let mut out = Self {
            value: VecPtr::from_vec(parts),
        };
        out.trim();
        out
    }

    /// NOTE: This is guranteed to generate the minimal number of bytes to
    /// represent the number with the sign bit.
    pub fn to_be_bytes(&self) -> Vec<u8> {
        if self.value.len() == 0 {
            return vec![];
        }

        let mut out = vec![];

        // Minimally encode most significant bytes
        let data = self.value.last().unwrap().to_be_bytes();
        let mut start_i = 0;
        while start_i < data.len() - 1
            && ((data[start_i] == 0xff && data[start_i + 1] & 0x80 != 0)
                || (data[start_i] == 0x00 && data[start_i + 1] & 0x80 == 0))
        {
            start_i += 1;
        }

        out.extend_from_slice(&data[start_i..]);

        // Output all the remaining bytes
        for i in (0..(self.value.len() - 1)).rev() {
            out.extend_from_slice(&self.value[i].to_be_bytes());
        }

        out
    }

    pub fn nbits(&self) -> usize {
        let mut n = 32 * self.value.len();
        if n == 0 {
            return 0;
        }

        let last = self.value.last().cloned().unwrap_or(0);
        if self.is_positive() {
            n -= last.leading_zeros() as usize
        } else {
            n -= (!last).leading_zeros() as usize
        }

        // Need one bit for the sign.
        n += 1;

        n
    }

    pub fn from_isize(v: isize) -> Self {
        Self::from_le_bytes(&v.to_le_bytes())
    }

    pub fn to_isize(&self) -> Result<isize> {
        if self.value.len() == 0 {
            return Ok(0);
        }

        let val = self.value.as_ref();
        if val.len() > 2 {
            return Err(err_msg("Integer too large"));
        }

        // TODO: This assumes we are on a 64-bit system
        let mut v = val.get(0).cloned().unwrap_or(0) as u64;
        v |= (val.get(1).cloned().unwrap_or(0) as u64) << 32;
        Ok(v as isize)
    }

    pub fn to_uint(&self) -> Result<BigUint> {
        if !self.is_positive() {
            return Err(err_msg("Not positive"));
        }

        // TODO: Use little endian / native endian.
        Ok(BigUint::from_be_bytes(&self.to_be_bytes()))
    }

    fn trim(&mut self) {
        const MAX: u32 = 0xffffffff;

        let val = self.value.as_mut();
        while val.len() >= 2
            && ((val[val.len() - 1] == 0 && val[val.len() - 2] & (1 << 31) == 0)
                || (val[val.len() - 1] == MAX && val[val.len() - 2] & (1 << 31) != 0))
        {
            val.pop();
        }

        if val.len() == 1 && val[0] == 0 {
            val.pop();
        }
    }

    pub fn is_positive(&self) -> bool {
        self.value.last().cloned().unwrap_or(0) & (1 << 31) == 0
    }

    pub fn is_zero(&self) -> bool {
        self.value.len() == 0
    }

    pub fn is_one(&self) -> bool {
        self.value.len() == 1 && self.value[0] == 1
    }
}

impl std::convert::From<BigUint> for BigInt {
    fn from(other: BigUint) -> Self {
        // TODO: We should be able to do this even more efficiently without a
        // conversion as the internal representations are the same.

        let mut data = other.to_le_bytes();
        data.push(0);

        Self::from_le_bytes(&data)
    }
}

impl std::fmt::Debug for BigInt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Ok(v) = self.to_isize() {
            write!(f, "{}", v)
        } else if self.is_positive() {
            write!(f, "{:?}", self.to_uint().unwrap())
        } else {
            // TODO: Need a better mode for this.
            write!(f, "BigInt({:?})", self.value.as_ref())
        }
    }
}
