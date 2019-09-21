// TLS specific helpers for parsing binary packets.

use parsing::*;
use parsing::binary::*;
use bytes::Bytes;

pub const U8_LIMIT: usize = u8::max_value() as usize;
pub const U16_LIMIT: usize = u16::max_value() as usize;
pub const U24_LIMIT: usize = 1 << 24;
pub const U32_LIMIT: usize = u32::max_value() as usize;

pub fn exp2(v: usize) -> usize {
	1 << v
}

/// Creates a parser for a variable length vector of bytes.
/// The max_bytes will be used to determine how large the 
pub fn varlen_vector(min_bytes: usize, max_bytes: usize) -> impl Parser<Bytes> {
	seq!(c => {
		let len =
			if max_bytes <= U8_LIMIT {
				c.next(be_u8)? as usize
			} else if max_bytes <= U16_LIMIT {
				c.next(be_u16)? as usize
			} else if max_bytes <= U24_LIMIT {
				panic!("u24 not implemented for the length");
			} else if max_bytes <= U32_LIMIT {
				c.next(be_u32)? as usize
			} else {
				panic!("Maximum length not supported");
			};
		if len < min_bytes || len > max_bytes {
			return Err("Length out of allowed range".into());
		}

		let data = c.next(take_exact(len))?;
		Ok(data)
	})
}
