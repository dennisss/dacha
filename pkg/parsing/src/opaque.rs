use std::convert::From;
use std::fmt::Write;

use common::errors::*;
use common::bytes::Bytes;

/// A set of bytes that has no restrictions on byte values, but most likely
/// contains visible ASCII characters.
#[derive(Clone, PartialEq)]
pub struct OpaqueString {
    data: Bytes
}

impl OpaqueString {
    pub fn new() -> Self {
        Self { data: Bytes::new() }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// NOTE: This is a relatively cheap operation.
    pub fn to_bytes(&self) -> Bytes {
        self.data.clone()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Attempts to interpret this string's contents as ASCII.
    pub fn to_ascii_str(&self) -> Result<&str> {
        if !self.data.as_ref().is_ascii() {
            return Err(err_msg("Not ascii"));
        }

        Ok(std::str::from_utf8(&self.data).unwrap())
    }
    
    pub fn to_utf8_str(&self) -> Result<&str> {
        Ok(std::str::from_utf8(&self.data)?)
    }
}

impl<T: Into<Bytes>> From<T> for OpaqueString {
    fn from(data: T) -> Self {
        Self { data: data.into() }
    }
}

// impl From<String> for OpaqueString {
//     fn from(data: String) -> Self {
//         Self { data: Bytes::from(data) }
//     }
// }

// impl From<Vec<u8>> for OpaqueString {
//     fn from(data: Vec<u8>) -> Self {
//         Self { data: Bytes::from(data) }
//     }
// }

// impl From<Bytes> for OpaqueString {
//     fn from(data: Bytes) -> Self {
//         Self { data }
//     }
// }

// impl From<&str> for OpaqueString {
//     fn from(s: &str) -> Self {
//         Self { data: Bytes::from(s) }
//     }
// }

impl std::fmt::Debug for OpaqueString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.data.iter().cloned() {
            // TODO: Limit this to only visible 
            if byte == b'\\' {
                write!(f, "\\\\")?;
            } else if byte.is_ascii() {
                f.write_char(byte as char);
            } else {
                write!(f, "\\x{:02X}", byte)?
            }
        }

        Ok(())
    }
}