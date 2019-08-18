use std::io::{Seek, Read};
use common::errors::*;
use byteorder::{ReadBytesExt, LittleEndian, ByteOrder};
use std::convert::{TryFrom, TryInto};
use parsing::iso::*;
use crate::crc::*;

// ZLib RFC http://www.zlib.org/rfc-gzip.html
// This is based on v4.3

// Also http://www.onicos.com/staff/iz/formats/gzip.html

// ISO 8859-1 (LATIN-1) strings
// 

// TODO: See https://github.com/Distrotech/gzip/blob/94cfaabe3ae7640b8c0334283df37cbdd7f7a0a9/gzip.h#L161 for new and old magic bytes



#[derive(Debug)]
pub enum CompressionMethod {
	// Stored = 0,
	// Compressed = 1,
	// Packed = 2,
	// LZH = 3,
	// 4..7 reserved

	Reserved = 0, // 0..7
	Deflate = 8
}

impl std::convert::TryFrom<u8> for CompressionMethod {
	type Error = common::errors::Error;
	fn try_from(i: u8) -> Result<Self> {
		Ok(match i {
			0|1|2|3|4|5|6|7 => CompressionMethod::Reserved,
			8 => CompressionMethod::Deflate,
			_ => { return Err("Unknown compression method".into()); }

		})
	}
}

#[derive(Debug)]
pub struct Flags {
	// Whether the text is probably ASCII.
	ftext: bool,
	// CRC16 of header is present.
	fhcrc: bool,
	fextra: bool,
	fname: bool,
	fcomment: bool
}

fn bitget(v: u8, bit: u8) -> bool {
	if v & (1 << bit) != 0 {
		true
	} else {
		false
	}
}

#[derive(Debug)]
pub struct Header {
	pub compression_method: CompressionMethod,
	pub flags: Flags,
	/// Seconds since unix epoch.
	pub mtime: u32,
	pub extra_flags: u8,
	pub os: u8,
	//
	pub extra_field: Option<Vec<u8>>,
	pub fname: Option<String>,
	pub comment: Option<String>,
	// Whether or not a checksum was present for the header (to validate all of the above fields).
	// NOTE: If the checksum failed, then this struct would be have been emited
	pub header_validated: bool
}

#[derive(Debug)]
pub struct GZipFile {
	pub header: Header,

	/// File byte offsets 
	pub compressed_range: (u64, u64),

	/// CRC32 of above compressed data range.
	pub compressed_checksum: u32,

	/// Uncompressed size (mod 2^32) of the original input to the compressor.
	pub input_size:  u32
}



// TODO: Set a save max limit on length.
fn read_null_terminated(reader: &mut Read) -> Result<Vec<u8>> {
	let mut out = vec![];
	let mut buf = [0u8; 1];
	
	loop {
		let n = reader.read(&mut buf)?;
		if n == 0 {
			return Err("Hit end of file before seeing null terminator".into());
		}

		if buf[0] == 0 {
			break;
		} else {
			out.push(buf[0]);
		}
	}

	Ok(out)
}

pub fn read_gzip<F: Read + Seek>(f: &mut F) -> Result<GZipFile> {

	// TODO: Verify compression properties such as maximum code length.

	// let mut txtf = File::open("testdata/lorem_ipsum.txt")?;
	// let mut data = Vec::new();
	// txtf.read_to_end(&mut data)?;
	// let mut c = CRC32Hasher::new();
	// c.write(&data);
	// println!("Uncompressed CRC32: {:x}", c.finish());

	let mut header_reader = CRC32Reader::new(f);

	let mut header_buf = [0u8; 10];
	header_reader.read_exact(&mut header_buf)?;

	if &header_buf[0..2] != &[0x1f, 0x8b] {
		return Err("Invalid header bytes".into());
	}

	let compression_method = CompressionMethod::try_from(header_buf[2])?;

	let flags = {
		let i = header_buf[3];
		Flags {
			ftext: bitget(i, 0),
			fhcrc: bitget(i, 1),
			fextra: bitget(i, 2),
			fname: bitget(i, 3),
			fcomment: bitget(i, 4)
		}
	};

	let mtime = LittleEndian::read_u32(&header_buf[4..8]);

	let extra_flags = header_buf[8];
	let os = header_buf[9];

	let extra_field = if flags.fextra {
		let xlen = header_reader.read_u32::<LittleEndian>()? as usize;
		let mut field = vec![];
		field.resize(xlen, 0);
		header_reader.read_exact(&mut field)?;
		Some(field)
	} else {
		None
	};

	let fname = if flags.fname {
		let data = read_null_terminated(&mut header_reader)?;
		Some(ISO88591String::from_bytes(data.into())?.to_string())
	} else {
		None
	};

	let comment = if flags.fcomment {
		let data = read_null_terminated(&mut header_reader)?;
		Some(ISO88591String::from_bytes(data.into())?.to_string())
	} else {
		None
	};

	let header_sum = header_reader.finish();
	drop(header_reader);

	let header_validated = if flags.fhcrc {
		let stored_checksum = f.read_u16::<LittleEndian>()?;
		println!("{:x} {:x}", header_sum, stored_checksum);

		// TODO: Compare it

		true
	} else {
		false
	};

	let body_start = f.seek(std::io::SeekFrom::Current(0))?;
	let file_length = f.seek(std::io::SeekFrom::End(0))?;
	let body_end = file_length - 8; // 8 is the size of the two fields in the footer

	if body_end < body_start {
		return Err("GZip stream too short: Missing footer".into());
	}

	f.seek(std::io::SeekFrom::Start(body_end))?;
	let body_checksum = f.read_u32::<LittleEndian>()?;
	let input_size = f.read_u32::<LittleEndian>()?;

	Ok(GZipFile {
		header: Header {
			compression_method,
			flags,
			mtime,
			extra_flags,
			os,
			extra_field,
			fname,
			comment,
			header_validated
		},
		compressed_range: (body_start, body_end),
		compressed_checksum: body_checksum,
		input_size
	})
}
