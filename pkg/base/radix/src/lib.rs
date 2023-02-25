#![feature(const_for)]
#![no_std]

#[cfg(feature = "std")]
#[macro_use]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[macro_use]
extern crate failure;

mod base32;
mod base64;

pub use base32::*;
pub use base64::*;

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Fail, Clone, Copy)]
pub struct DecodeRadixError {
    pub input_position: usize,
    pub kind: DecodeRadixErrorKind,
}

impl core::fmt::Display for DecodeRadixError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DecodeRadixErrorKind::{:?} at position {}",
            self.kind, self.input_position
        )
    }
}

#[derive(Clone, Copy, Debug)]
pub enum DecodeRadixErrorKind {
    InvalidNumberOfDigits,
    UnsupportedDigit,
}

pub fn hex_encode(data: &[u8]) -> String {
    let mut out = String::new();
    out.reserve_exact(data.len() * 2);
    for b in data.iter().cloned() {
        out.push(char::from_digit((b >> 4) as u32, 16).unwrap());
        out.push(char::from_digit((b & 0b1111) as u32, 16).unwrap());
    }

    out
}

// TODO: Return Result<..>
pub fn hex_decode(text: &str) -> Result<Vec<u8>, DecodeRadixError> {
    let mut out = vec![];

    let mut digit = String::new();
    let mut num_chars = 0;
    let mut input_position = 0;

    for c in text.chars() {
        digit.push(c);
        num_chars += 1;

        if num_chars == 2 {
            out.push(
                u8::from_str_radix(&digit, 16).map_err(|_| DecodeRadixError {
                    input_position,
                    kind: DecodeRadixErrorKind::UnsupportedDigit,
                })?,
            );
            digit.clear();
            num_chars = 0;
        }

        input_position += 1;
    }

    if num_chars != 0 {
        return Err(DecodeRadixError {
            input_position,
            kind: DecodeRadixErrorKind::InvalidNumberOfDigits,
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_decode_test() {
        assert_eq!(&hex_decode("AB").unwrap(), &[0xAB]);
        assert_eq!(&hex_decode("12").unwrap(), &[0x12]);
        assert_eq!(&hex_decode("aabb").unwrap(), &[0xAA, 0xBB]);
        assert_eq!(
            &hex_decode("123456789A").unwrap(),
            &[0x12, 0x34, 0x56, 0x78, 0x9A]
        );
    }
}
