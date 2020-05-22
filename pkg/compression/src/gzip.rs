use std::io::{Seek, Read, Write};
use common::errors::*;
use common::bits::{bitget, bitset};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian, ByteOrder};
use std::convert::{TryFrom, TryInto};
use parsing::iso::*;
use crypto::hasher::*;
use crypto::checksum::crc::*;
use crate::deflate::*;

// ZLib RFC http://www.zlib.org/rfc-gzip.html
// This is based on v4.3

// Also http://www.onicos.com/staff/iz/formats/gzip.html

// ISO 8859-1 (LATIN-1) strings
// 

// TODO: See https://github.com/Distrotech/gzip/blob/94cfaabe3ae7640b8c0334283df37cbdd7f7a0a9/gzip.h#L161 for new and old magic bytes



#[derive(Debug, PartialEq)]
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
			_ => { return Err(err_msg("Unknown compression method")); }

		})
	}
}


#[derive(Debug)]
pub struct Flags {
	// Whether the text is probably ASCII.
	ftext: bool,
	// CRC16 of header is present.
	fhcrc: bool,
	// Extra field is present.
	fextra: bool,
	// Filename is present
	fname: bool,
	// Comment is present.
	fcomment: bool
}

impl Flags {
	pub fn from_byte(i: u8) -> Flags {
		Flags {
			ftext: bitget(i, 0),
			fhcrc: bitget(i, 1),
			fextra: bitget(i, 2),
			fname: bitget(i, 3),
			fcomment: bitget(i, 4)
		}
	}

	pub fn to_byte(&self) -> u8 {
		let mut i = 0;
		bitset(&mut i, self.ftext, 0);
		bitset(&mut i, self.fhcrc, 1);
		bitset(&mut i, self.fextra, 2);
		bitset(&mut i, self.fname, 3);
		bitset(&mut i, self.fcomment, 4);
		i
	}
}

#[derive(Debug)]
pub struct Header {
	pub compression_method: CompressionMethod,

	// Whether the text is probably ASCII.
	pub is_text: bool,

	/// Seconds since unix epoch.
	pub mtime: u32,
	// TODO: Check what is supposed to be in here.
	pub extra_flags: u8,
	pub os: u8,
	//
	pub extra_field: Option<Vec<u8>>,
	pub filename: Option<String>,
	pub comment: Option<String>,
	// Whether or not a checksum was present for the header (to validate all of the above fields).
	// NOTE: If the checksum failed, then this struct would be have been emited
	pub header_validated: bool
}

#[derive(Debug)]
pub struct GZipFile {
	pub header: Header,

	pub data: Vec<u8>

	/*
	/// File byte offsets 
	pub compressed_range: (u64, u64),

	/// CRC32 of above compressed data range.
	pub compressed_checksum: u32,

	/// Uncompressed size (mod 2^32) of the original input to the compressor.
	pub input_size:  u32
	*/
}



// TODO: Set a save max limit on length.
fn read_null_terminated(reader: &mut dyn Read) -> Result<Vec<u8>> {
	let mut out = vec![];
	let mut buf = [0u8; 1];
	
	loop {
		let n = reader.read(&mut buf)?;
		if n == 0 {
			return Err(err_msg("Hit end of file before seeing null terminator"));
		}

		if buf[0] == 0 {
			break;
		} else {
			out.push(buf[0]);
		}
	}

	Ok(out)
}

const HEADER_SIZE: usize = 10;

pub const GZIP_UNIX_OS: u8 = 0x03;

const GZIP_MAGIC: &'static [u8] = &[0x1f, 0x8b];

// Most trivial to do this with a buffer because then we can perform buffering 

// Reader will buffer all input until 

enum GzipDecodeState {
	/// Very start of the file including all conditional fields
	Header,
	
	/// This will need to have an Inflater, and a rolling checksum
	Body {  },

	Footer
}

struct GzipDecoder {



}

impl GzipDecoder {

	// /// Will return non-None once 
	// pub fn header(&self) -> Option<&Header> {

	// }


}


/*
	Reader wrapper:
	- Implements Read
	- As it reads, it internally caches all read data
	- Internal cache is discarded on called 
*/

/// A reader which can be at any time 'rolled' back such it 
trait RecordedRead: Read {
	/// Should be called to indicate that all internal cache 
	fn consume(&mut self);

	fn take(&mut self) -> Vec<u8>;
}

// struct SliceReader {

// }

// impl RecordedRead for std::io::Cursor<[u8]> {

// }

// TODO: Convert this to a state machine.
// TODO: Can't seek a TcpStream
pub fn read_gzip<F: Read>(f: &mut F) -> Result<GZipFile> {

	// TODO: Verify compression properties such as maximum code length.

	let mut header_reader = HashReader::new(f, CRC32Hasher::new());

	let mut header_buf = [0u8; HEADER_SIZE];
	header_reader.read_exact(&mut header_buf)?;

	if &header_buf[0..2] != GZIP_MAGIC {
		return Err(err_msg("Invalid header bytes"));
	}

	let compression_method = CompressionMethod::try_from(header_buf[2])?;

	let flags = Flags::from_byte(header_buf[3]);

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

	let filename = if flags.fname {
		let data = read_null_terminated(&mut header_reader)?;
		Some(Latin1String::from_bytes(data.into())?.to_string())
	} else {
		None
	};

	let comment = if flags.fcomment {
		let data = read_null_terminated(&mut header_reader)?;
		Some(Latin1String::from_bytes(data.into())?.to_string())
	} else {
		None
	};

	let header_sum = header_reader.into_hasher().finish_u32();

	let header_validated = if flags.fhcrc {
		let stored_checksum = f.read_u16::<LittleEndian>()?;
		println!("{:x} {:x}", header_sum, stored_checksum);

		// TODO: Compare it

		true
	} else {
		false
	};

	let mut checksum_reader = HashReader::new(f, CRC32Hasher::new());

	let uncompressed_data =
		if compression_method == CompressionMethod::Deflate {
			checksum_reader.read_inflate()?
		} else {
			return Err(err_msg("Unsupported compression method"));
		};

	let actual_checksum = {
		let mut hasher = CRC32Hasher::new();
		hasher.update(&uncompressed_data);
		hasher.finish_u32()
	};

	let body_checksum = f.read_u32::<LittleEndian>()?;
	let input_size = f.read_u32::<LittleEndian>()? as usize;

	if input_size != uncompressed_data.len() {
		return Err(format_err!(
			"Footer length mismatch, expected: {}, actual: {}",
			uncompressed_data.len(), input_size));
	}

	if body_checksum != actual_checksum {
		return Err(err_msg("Footer wrong checksum"));
	}

	// TODO: If reading from a file, validate that we are at the end after parsing it

	Ok(GZipFile {
		header: Header {
			compression_method,
			is_text: flags.ftext,
			mtime,
			extra_flags,
			os,
			extra_field,
			filename,
			comment,
			header_validated
		},
		data: uncompressed_data
	})
}

// TODO: Must operate on the uncompressed data.
fn is_text(data: &[u8]) -> bool {
	for b in data.iter().cloned() {
		if b == 9 || b == 10 || b == 13 || (b >= 32 && b <= 126) {
			// Good. Keep going.
		} else {
			return false;
		}
	}

	true
}

pub fn write_gzip(header: Header, data: &[u8],
				  writer: &mut dyn Write) -> Result<()> {
	let flags = Flags {
		ftext: false,
		fhcrc: false,
		fextra: header.extra_field.is_some(),
		fname: header.filename.is_some(),
		fcomment: header.comment.is_some()
	};

	let mut header_buf = [0u8; HEADER_SIZE];
	header_buf[0..2].copy_from_slice(GZIP_MAGIC);
	header_buf[2] = header.compression_method as u8;
	header_buf[3] = flags.to_byte();
	LittleEndian::write_u32(&mut header_buf[4..8], header.mtime);
	header_buf[8] = header.extra_flags;
	header_buf[9] = header.os;
	writer.write_all(&header_buf)?;

	if let Some(data) = header.extra_field {
		writer.write_u32::<LittleEndian>(data.len() as u32)?;
		writer.write_all(&data)?;
	}

	let null = [0u8; 1];
	if let Some(s) = header.filename {
		// TODO: Validate is Latin1String.
		writer.write_all(s.as_bytes())?;
		writer.write_all(&null)?;
	}

	if let Some(s) = header.comment {
		// TODO: Validate is Latin1String.
		writer.write_all(s.as_bytes())?;
		writer.write_all(&null)?;
	}

	// TODO: Validate that the compresson method is set correctly.
	let mut deflater = Deflater::new();
	deflater.update(&data, &mut [], true)?;
	let compressed_data = deflater.take_output();

	println!("{:?}", compressed_data);

	writer.write_all(&compressed_data)?;

	let mut hasher = CRC32Hasher::new();
	hasher.update(&data);
	let checksum = hasher.finish_u32();

	writer.write_u32::<LittleEndian>(checksum)?;
	writer.write_u32::<LittleEndian>(data.len() as u32)?;
	Ok(())
}

