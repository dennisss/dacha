// Utilities for dealing for sets of bits and bit stream I/O.

use std::io::{Read, Write};
use crate::errors::*;
use crate::ceil_div;

/// Represents a variable length number of ordered bits
#[derive(PartialEq, Eq, Clone)]
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

	pub fn clear(&mut self) {
		self.data.clear();
		self.len = 0;
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
	pub fn get(&self, i: usize) -> Option<u8> {
		if i >= self.len {
			return None;
		}

		Some((self.data[i / 8] >> (7 - (i % 8))) & 0b1)
	}

	/// Generates a bitvector from a number. The corresponding vector will start with the MSB of the number.
	pub fn from_usize(val: usize, width: u8) -> Self {
		let mut out = BitVector::new();
		for i in 0..width { // NOTE: THis is not reversed!
			out.push(((val >> i) & 0b1) as u8)
		}
		
		// Assert 'val' has no more than width data in it.
		assert_eq!(val >> width, 0);

		out
	}

	pub fn from(data: &[u8], len: usize) -> Self {
		let mut data = Vec::from(data);
		data.resize(ceil_div(len, 8), 0);

		Self { data, len }
	}
}

// TODO: THis will be wrong if we don't have a number of bits divisble by 8.
// ^ AKA: '1' should be encoded as '1' instead of as '0x80'
// NOTE: This should be guranteed to always minimally cover all bits up to the
// next complete octet.
impl std::convert::AsRef<[u8]> for BitVector {
	fn as_ref(&self) -> &[u8] {
		&self.data
	}
}

impl std::fmt::Debug for BitVector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mut s = String::new();
		for i in 0..self.len() {
			s += &self.get(i).unwrap().to_string();
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
pub struct BitReader<'a> {
	reader: &'a mut dyn Read,
	// Offset from 0-N bits within the buffer
	// Usually N will be 7 if no errors occur which would cause more than 8 bits to be buffered.
	offset: usize,

	// How many bits were consumed (aka we can drop all bits before this point)
	consumed_offset: usize,
	//
	buffer: BitVector,
}

// NOTE: THis reads from 
impl<'a> BitReader<'a> {
	pub fn new(reader: &'a mut dyn Read) -> Self {
		BitReader { reader, offset: 0, buffer: BitVector::new(),
					consumed_offset: 0 }
	}

	pub fn load(&mut self, bits: BitVector) -> Result<()> {
		if self.offset != self.buffer.len() {
			return Err("Already have pending bits loaded".into());
		}
		
		self.buffer = bits;

		Ok(())
	}

	// TODO: Must support reading usize to read the lengths
	// TODO: This is heavily biased towards how zlib does it
	/// Reads a given number of bits from the stream and returns them as a byte.
	/// Up to 8 bits can be read.
	/// The final bit read will be in the most significant position of the return value.
	/// 
	/// NOTE: Unless consume() is called, then this will accumulate bits indefinately
	/// 
	/// NOTE: If an BitIoErrorKind::NotEnoughBits error occurs, then this operation is retryable if the reader later has all of the remaining bits.
	/// 
	/// The return value will be None if and only if the first read bit is after the end of the file.
	pub fn read_bits(&mut self, n: u8) -> Result<Option<usize>> {

		// TODO: Can be implemented as a trivial read
		// But reading more than 8 bits can be tricky. Basially must loop through bytes instead of through bits
		// if n < 8 - self.bit_offset {
		// 	let mask = (1 << n) - 1;

		// }

		// TODO: Instead implement as a read from up to two bytes.
		let mut out = 0;
		for i in 0..n {
			if self.offset == self.buffer.len() {
				let mut buf = [0u8; 1];
				let nread = self.reader.read(&mut buf)?;
				if nread == 0 {
					if i == 0 {
						return Ok(None);
					} else {
						// Rollback and store all the bits we've read.
						// TODO: In this case, reset the offset?

						return Err(ErrorKind::BitIo(
							BitIoErrorKind::NotEnoughBits).into());
					}
				} else {
					// Push bits into buffer from LSB to MSB
					let mut b = buf[0];
					for _ in 0..8 {
						self.buffer.push(b & 0b01);
						b = b >> 1;
					}
				}
			}

			out = out | ((self.buffer.get(self.offset).unwrap() as usize) << i);
			self.offset += 1;
		}

		Ok(Some(out))
	}

	pub fn read_bits_exact(&mut self, n: u8) -> Result<usize> {
		// TODO: This error should also be identified.
		self.read_bits(n)?.ok_or(ErrorKind::BitIo(
							BitIoErrorKind::NotEnoughBits).into())
			
			//Error::from("Hit end of file during read"))
	}

	pub fn consume(&mut self) {
		if self.offset == self.buffer.len() {
			self.buffer.clear();
			self.offset = 0;
		} else {
			self.consumed_offset = self.offset;
		}
	}

	/// Moves the cursor of the stream to the next full byte
	pub fn align_to_byte(&mut self) {
		let r = self.offset % 8;
		if r != 0 {
			self.offset += 8 - r;
		}
	}

	// Outputs all remaining unread bits in the last read bytes.
	pub fn into_unconsumed_bits(self) -> BitVector {
		let mut buf = BitVector::new();
		for i in self.consumed_offset..self.buffer.len() {
			buf.push(self.buffer.get(i).unwrap());
		}

		buf
	}
}

/*
impl<T> BitReader<'_, std::io::Cursor<T>> {
	///
	/// 
	/// Returns:
	/// (# of full bytes read,
	///  # of bits read in the next byte after those)
	fn position(&self) -> (usize, usize) {
		let mut nbytes = self.reader.position();
		if self.bit_offset > 0 {
			nbytes -= 1;
		}

		(nbytes, self.bit_offset)
	}
}
*/

impl Read for BitReader<'_> {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		// We do not buffer unconsumed bits when reading full bytes, so 
		// NOTE: This would also check for 'self.buffer.len() != self.offset'
		if self.buffer.len() != self.consumed_offset {
			return Err(std::io::Error::new(std::io::ErrorKind::Other, "Reading would drop trailing bits"));
		}

		self.reader.read(buf)
	}
}

pub trait BitWrite {
	/// Writes the lowest 'len' bits of 'val' to this stream.
	fn write_bits(&mut self, val: usize, len: u8) -> Result<()>;

	fn write_bitvec(&mut self, val: &BitVector) -> Result<()> {
		for i in 0..val.len() {
			self.write_bits(val.get(i).unwrap() as usize, 1)?;
		}

		Ok(())
	}

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

	/// Obtains a bitvector that represents all pending bits inside of the writer.
	/// Calling write_bitvec() later on an empty BitWrite will return the BitWrite to the same state.
	pub fn into_bits(self) -> BitVector {
		let mut out = BitVector::new();
		let mut v = self.current_byte;
		for i in 0..self.bit_offset {
			out.push((v & 0b1) as u8);
			v = v >> 1;
		}

		out
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

		// Ensure that 'val' doesn't contain more the 'len' bits
		assert_eq!(val, 0);

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

	#[test]
	fn bitwriter_test() {
		let mut data = Vec::new();
		let mut strm = BitWriter::new(&mut data);
		strm.write_bits(0b1, 1).unwrap();
		strm.write_bits(0b01, 2).unwrap();
		strm.finish().unwrap();

		assert_eq!(data[0], 0b011);
	}

}
