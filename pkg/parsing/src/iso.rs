
use bytes::Bytes;
use common::errors::*;

// ISO-8859-1 string reference
// (https://en.wikipedia.org/wiki/ISO/IEC_8859-1)
pub struct ISO88591String {
	// All bytes must be in the ranges:
	// - [0x20, 0x7E]
	// - [0xA0, 0xFF] 
	pub data: Bytes
}

impl ISO88591String {
	pub fn from(s: &str) -> Result<ISO88591String> {
		let mut data = vec![];
		for c in s.chars() {
			let v = c as usize;
			if v > 0xff {
				return Err("Char outside of single byte range".into());
			}

			data.push(v as u8);
		}

		ISO88591String::from_bytes(Bytes::from(data))
	}

	pub fn from_bytes(data: Bytes) -> Result<ISO88591String> {
		for i in &data {
			let valid = (*i >= 0x20 && *i <= 0x7e) ||
						(*i >= 0xa0);
			if !valid {
				return Err(
					format!("Undefined ISO-8859-1 code point: {:x}", i).into());
			}
		}

		Ok(ISO88591String { data })
	}

	/// Converts to a standard utf-8 string.
	pub fn to_string(&self) -> String {
		let mut s = String::new();
		for i in &self.data {
			let c = std::char::from_u32(*i as u32).expect("Invalid character");
			s.push(c);
		}

		s
	}
}

impl std::fmt::Debug for ISO88591String {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.to_string().fmt(f)
    }
}
