
// Implementation of ZLIB compressed data format as described in https://www.ietf.org/rfc/rfc1950.txt
// No relation to the zlib C library.

// Big endian integers

use std::io::{Read, Write};
use std::convert::{TryFrom, TryInto};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use common::errors::*;
use crypto::hasher::*;
use crate::deflate::*;

struct DeflateInfo {
	/// LZ77 window size used by the compressor in bytes.
	window_size: usize
}

enum CompressionMethod {
	Deflate(DeflateInfo)
}

enum CompressionLevel {
	Fastest = 0,
	Fast = 1,
	Default = 2,
	/// Maximum compression
	Slowest = 3
}

impl TryFrom<u8> for CompressionLevel {
	type Error = Error;
	fn try_from(v: u8) -> Result<CompressionLevel> {
		Ok(match v {
			0 => CompressionLevel::Fastest,
			1 => CompressionLevel::Fast,
			2 => CompressionLevel::Default,
			3 => CompressionLevel::Slowest,
			// NOTE: Will never happen as we will always use a 4 bit integer.
			_ => { return Err("Invalid compression level".into()); }
		})
	}
}

const WINDOW_LOG_OFFSET: u8 = 8;

impl CompressionMethod {
	fn decode(cmf: u8) -> Result<CompressionMethod> {
		let cm = cmf & 0b1111;
		let cinfo = cmf >> 4;

		Ok(match cm {
			8 => {
				let size = 1 << (cinfo + WINDOW_LOG_OFFSET) as usize;
				if size > 32768 {
					return Err("Window size too large for deflate".into());
				}

				CompressionMethod::Deflate(DeflateInfo {
					window_size: size
				})
			},
			_ => { return Err("Unknown compression method".into()); }
		})
	}
}


const ADLER32_PRIME_MOD: usize = 65521;

struct Adler32Hasher {
	// NOTE: Must be >16bits each
	s1: usize,
	s2: usize
}

impl Adler32Hasher {
	pub fn new() -> Adler32Hasher {
		Adler32Hasher::from_hash(1)
	}

	pub fn from_hash(hash: u32) -> Adler32Hasher {
		Adler32Hasher {
			s1: (hash & 0xffff) as usize,
			s2: ((hash >> 16) & 0xffff) as usize
		}
	}

	pub fn finish_u32(&self) -> u32 {
		((self.s2 << 16) | self.s1) as u32
	}
}

impl Hasher for Adler32Hasher {
	fn output_size(&self) -> usize { 4 }

	fn update(&mut self, data: &[u8]) {
		for v in data.iter().cloned() {
			self.s1 = (self.s1 + (v as usize)) % ADLER32_PRIME_MOD;
			self.s2 = (self.s2 + self.s1) % ADLER32_PRIME_MOD;
		}
	}

	fn finish(&self) -> Vec<u8> {
		self.finish_u32().to_be_bytes().to_vec()
	}
}


struct Zlib {
	compression_method: CompressionMethod,
	compression_level: CompressionLevel,
	// Adler32 of the dictionary being used.
	dictid: Option<u32>
}

pub fn read_zlib(mut reader: &mut dyn Read) -> Result<Vec<u8>> {
	let mut header = [0u8; 2];
	reader.read_exact(&mut header)?;

	let cmf = header[0];
	let flg = header[1];
	if ((cmf as usize)*256 + (flg as usize)) % 31 != 0 {
		return Err("Invalid header bytes".into());
	}

	let compression_method = CompressionMethod::decode(cmf)?;
	let fcheck = flg & 0b1111; // Checked above.
	let fdict = (flg >> 5) & 0b1;
	let flevel = (flg >> 6) & 0b11;

	let dictid = if fdict == 1 {
		Some(reader.read_u32::<BigEndian>()?)
	} else {
		None
	};

	// TODO: Implement dictionary and pass in window size.
	let out = reader.read_inflate()?;

	// Checksum of uncompressed data.
	let mut hasher = Adler32Hasher::new();
	hasher.update(&out);
	let actual_checksum = hasher.finish_u32();

	let checksum = reader.read_u32::<BigEndian>()?;
	if checksum != actual_checksum {
		return Err("Invalid checksum".into());
	}

	Ok(out)
}

// TODO: Implement Write path
