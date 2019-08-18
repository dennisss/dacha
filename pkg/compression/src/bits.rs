// Utilities for dealing for sets of bits and bit stream I/O.

use std::io::{Read, Write};
use common::errors::*;

/// Represents a variable length number of ordered bits
#[derive(PartialEq, Eq)]
pub struct BitVector {
	// Bits are stored from MSB to LSB in each individual byte.
	data: Vec<u8>,
	len: usize
}

impl BitVector {
	/// Returns an empty vector.
	pub fn new() -> Self {
		BitVector { data: vec![], len: 0 }		
	}

	/// Appends a single bit to this vector.
	/// 'bit' must be 0 or 1
	pub fn push(&mut self, bit: u8) {
		assert!(bit <= 1);

		if self.len % 8 == 0 {
			self.data.push(0);
		}

		let last = self.data.last_mut().unwrap();
		*last |= bit << 7 - (self.len % 8);
		self.len += 1;
	}

	/// Get the total number of bits stored in this vector.
	pub fn len(&self) -> usize {
		self.len
	}

	/// Get a single bit from the vector where the index is in the same order as the bit was push'ed.
	pub fn get(&self, i: usize) -> u8 {
		assert!(i < self.len);
		(self.data[i / 8] >> (7 - (i % 8))) & 0b1
	}
}

impl std::fmt::Debug for BitVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mut s = String::new();
		for i in 0..self.len() {
			s += &self.get(i).to_string();
		}

        write!(f, "'{}'", s)
    }
}

/// Any string of '0' and '1' characters can be converted to a BitVector.
impl std::convert::TryFrom<&'_ str> for BitVector {
	type Error = Error;

    fn try_from(s: &str) -> Result<Self> {
		let mut out = BitVector::new();
		for c in s.chars() {
			if c == '0' {
				out.push(0);
			} else if c == '1' {
				out.push(1);
			} else {
				return Err(format!("Not 0|1: {}", c).into());
			}
		}

		Ok(out)
    }
}


/// Wrapper around a readable stream which allows for reading individual bits from the stream at a time.
/// 
/// For reading many bytes, Read is also implemented, but it is invalid to use Read 
pub struct BitStream<'a> {
	reader: &'a mut dyn Read,
	// Offset from 0-7 within the current 
	bit_offset: u8,
	current_byte: Option<u8>
}

impl<'a> BitStream<'a> {
	pub fn new(reader: &'a mut dyn Read) -> Self {
		BitStream { reader, bit_offset: 0, current_byte: None }
	}

	// TODO: Must support reading usize to read the lengths
	// TODO: This is heavily biased towards how zlib does it
	/// Reads a given number of bits from the stream and returns them as a byte.
	/// Up to 8 bits can be read.
	/// The final bit read will be in the most significant position of the return value.
	/// 
	/// The return value will be None if and only if the first read bit is after the end of the file.
	pub fn read_bits(&mut self, n: u8) -> Result<Option<usize>> {
		// TODO: Instead implement as a read from up to two bytes.
		let mut out = 0;
		for i in 0..n {
			if self.current_byte.is_none() || self.bit_offset == 8 {
				let mut buf = [0u8; 1];
				let nread = self.reader.read(&mut buf)?;
				if nread == 0 {
					if i == 0 {
						return Ok(None);
					} else {
						return Err("Hit end of file in middle of read".into());
					}
				} else {
					self.current_byte = Some(buf[0]);
					self.bit_offset = 0;
				}
			}

			let b = self.current_byte.unwrap();
			out = out | (((b & 0b1) as usize) << i);
			
			self.current_byte = Some(b >> 1);
			self.bit_offset += 1;
		}

		Ok(Some(out))
	}

	pub fn read_bits_exact(&mut self, n: u8) -> Result<usize> {
		self.read_bits(n)?.ok_or(Error::from("Hit end of file during read"))
	}

	/// Moves the cursor of the stream to the next 
	pub fn align_to_byte(&mut self) {
		self.current_byte = None;
		self.bit_offset = 0;
	}
}

impl Read for BitStream<'_> {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		if self.current_byte.is_some() && self.bit_offset != 8 {
			return Err(std::io::Error::new(std::io::ErrorKind::Other, "Reading would drop trailing bits"));
		}

		self.reader.read(buf)
	}
}



pub trait BitWrite {
	/// Writes the lowest 'len' bits of 'val' to this stream.
	fn write_bits(&mut self, val: usize, len: u8) -> Result<()>;

	/// Immediately finish writing any partial bytes to the underlying stream.
	/// 
	/// NOTE: This should always be called after using this stream to guarantee that everything has been written.
	fn finish(&mut self) -> Result<()>;
}

pub struct BitWriter<'a> {
	writer: &'a mut dyn Write,
	bit_offset: u8,
	current_byte: u8
}

impl<'a> BitWriter<'a> {
	pub fn new(writer: &'a mut dyn Write) -> Self {
		BitWriter {
			writer,
			bit_offset: 0,
			current_byte: 0
		}
	}
}

impl BitWrite for BitWriter<'_> {
	fn write_bits(&mut self, mut val: usize, len: u8) -> Result<()> {
		for i in 0..len {
			self.current_byte |= ((val & 0b1) << self.bit_offset) as u8;
			self.bit_offset += 1;
			val = val >> 1;

			if self.bit_offset == 8 {
				self.finish()?;
			}
		}

		Ok(())
	}

	fn finish(&mut self) -> Result<()> {
		if self.bit_offset > 0 {
			let buf = [self.current_byte];
			self.writer.write_all(&buf)?;
			self.bit_offset = 0;
			self.current_byte = 0;
		}

		Ok(())
	}
}


#[cfg(test)]
mod tests {
	use super::*;


	#[test]
	fn bitvector_works() {
		let mut v = BitVector::new();
		let vals = vec![0, 1, 1, 0, 1, 0, 1, 1, 1, 0, 0];
		for i in 0..vals.len() {
			v.push(vals[i]);
			assert_eq!(v.len(), i + 1);
			for j in 0..(i + 1) {
				assert_eq!(v.get(j), vals[j]);
			}
		}

		assert_eq!(&format!("{:?}", v), "'01101011100'");
	}

}
