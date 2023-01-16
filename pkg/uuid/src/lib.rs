#![feature(const_for, const_mut_refs)]

extern crate automata;
#[macro_use]
extern crate common;
extern crate base_radix;
#[macro_use]
extern crate regexp_macros;

regexp!(PATTERN => "^([0-9a-fA-F]){8}-([0-9a-fA-F]){4}-([0-9a-fA-F]){4}-([0-9a-fA-F]){4}-([0-9a-fA-F]){12}$");

use core::fmt::{Debug, Formatter};

use common::errors::*;

#[derive(Clone)]
pub struct UUID {
    data: [u8; 16],
}

impl UUID {
    // TODO: Rename from_be_bytes
    pub const fn new(data: [u8; 16]) -> Self {
        Self { data }
    }

    /// NOTE: Only the first 10 btes are
    pub fn from_gpt_bytes(mut data: [u8; 16]) -> Self {
        data[0..4].reverse();
        data[4..6].reverse();
        data[6..8].reverse();

        Self { data }
    }

    pub fn parse(value: &str) -> Result<Self> {
        if !PATTERN.test(value) {
            return Err(err_msg("Invalid UUID format"));
        }

        let mut data = base_radix::hex_decode(&value.replace("-", ""))?;

        Ok(Self {
            data: *array_ref![data, 0, 16],
        })
    }

    pub fn to_string(&self) -> String {
        format!(
            "{}-{}-{}-{}-{}",
            base_radix::hex_encode(&self.data[0..4]),
            base_radix::hex_encode(&self.data[4..6]),
            base_radix::hex_encode(&self.data[6..8]),
            base_radix::hex_encode(&self.data[8..10]),
            base_radix::hex_encode(&self.data[10..])
        )
    }
}

impl AsRef<[u8]> for UUID {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}

impl Debug for UUID {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "UUID({})", self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_id() {
        let s = "123e4567-e89b-12d3-a456-426614174000";
        let id = UUID::parse(s).unwrap();

        assert_eq!(
            id.as_ref(),
            &[
                0x12, 0x3e, 0x45, 0x67, 0xe8, 0x9b, 0x12, 0xd3, 0xa4, 0x56, 0x42, 0x66, 0x14, 0x17,
                0x40, 0x00
            ]
        );

        assert_eq!(id.to_string(), "123e4567-e89b-12d3-a456-426614174000");
    }

    #[test]
    fn reject_invalid_ids() {
        let s = &[
            "",
            "123e4567-e89b-12d3-a456426614174000",
            "123e4567e89b-12d3-a456-426614174000",
            "123e4567e89b12d3a456426614174000",
            "123e4567-e89b-12d3-a456-42661417400",
            "1234",
        ];

        for s in s {
            assert!(UUID::parse(s).is_err());
        }
    }
}
