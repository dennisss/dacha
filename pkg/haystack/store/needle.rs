use super::super::common::*;
use super::super::errors::*;
use std::io;
use std::io::Cursor;
use std::io::{Write, Read};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use bytes::Bytes;
use crc32c::crc32c_append;
use arrayref::*;
use std::mem::size_of;




const CHECKSUM_SIZE: usize = 4;
const FOOTER_MAGIC: &str = "LES!";
const FOOTER_MAGIC_SIZE: usize = 4;

pub const NEEDLE_FOOTER_SIZE: usize =
	CHECKSUM_SIZE +
	FOOTER_MAGIC_SIZE;



const FLAG_DELETED: u8 = 1;



#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct NeedleKeys {
	pub key: u64,
	pub alt_key: u32
}

#[derive(Clone)]
pub struct NeedleMeta {
	pub flags: u8, // TODO: With padding, this will increase the memory footprint a lot
	pub size: u64
}

impl NeedleMeta {
	pub fn deleted(&self) -> bool {
		self.flags & FLAG_DELETED != 0
	}
}

// Basically all of this can be 
pub struct NeedleIndexEntry  {
	pub meta: NeedleMeta,

	pub block_offset: u32
}

impl NeedleIndexEntry {
	pub fn offset(&self) -> u64 {
		(self.block_offset as u64) * (BLOCK_SIZE as u64)
	}
}




const HEADER_MAGIC: &str = "NEED";
const HEADER_MAGIC_SIZE: usize = 4;
const COOKIE_SIZE: usize = size_of::<Cookie>();
const KEY_SIZE: usize = size_of::<NeedleKey>();
const ALT_KEY_SIZE: usize = size_of::<NeedleAltKey>();
const FLAG_SIZE: usize = 1;
const SIZE_SIZE: usize = 8;

pub const NEEDLE_HEADER_SIZE: usize =
	HEADER_MAGIC_SIZE +
	COOKIE_SIZE +
	KEY_SIZE +
	ALT_KEY_SIZE +
	FLAG_SIZE +
	SIZE_SIZE;

/// Offset from the start of the needle to the flags byte
pub const NEEDLE_FLAGS_OFFSET: usize =
	HEADER_MAGIC_SIZE +
	COOKIE_SIZE +
	KEY_SIZE +
	ALT_KEY_SIZE;

pub struct NeedleHeader {
	pub cookie: Cookie,
	pub keys: NeedleKeys,
	pub meta: NeedleMeta
}

impl NeedleHeader {

	pub fn read(reader: &mut Read) -> Result<NeedleHeader> {
		let mut buf = [0u8; NEEDLE_HEADER_SIZE];
		reader.read_exact(&mut buf)?;
		NeedleHeader::parse(&buf)
	}

	pub fn parse(header: &[u8; NEEDLE_HEADER_SIZE]) -> Result<NeedleHeader> {

		if &header[0..4] != HEADER_MAGIC.as_bytes() {
			return Err("Needle header magic is incorrect".into());
		}

		let mut pos = 4;
		let cookie = &header[pos..(pos + COOKIE_SIZE)]; pos += COOKIE_SIZE;
		let key = (&header[pos..]).read_u64::<LittleEndian>()?; pos += 8;
		let alt_key = (&header[pos..]).read_u32::<LittleEndian>()?; pos += 4;
		let flags = header[pos]; pos += 1;
		let size =  (&header[pos..]).read_u64::<LittleEndian>()?; pos += 8;

		// Ideally to be implemented in terms of 

		let mut n = NeedleHeader {
			cookie: [0u8; COOKIE_SIZE],
			keys: NeedleKeys {
				key, alt_key
			},
			meta: NeedleMeta {
				flags,
				size
			}
		};

		n.cookie.copy_from_slice(cookie);

		Ok(n)
	}

	// Annoyingly this will require a copy to do this 
	pub fn serialize(cookie: &Cookie, keys: &NeedleKeys, meta: &NeedleMeta) -> Result<Vec<u8>> {
		let mut data = Vec::new();
		data.reserve(NEEDLE_HEADER_SIZE);

		{
			let mut c = Cursor::new(&mut data);
			c.write_all(HEADER_MAGIC.as_bytes())?;
			c.write_all(cookie)?;
			c.write_u64::<LittleEndian>(keys.key)?;
			c.write_u32::<LittleEndian>(keys.alt_key)?;
			c.write_u8(meta.flags)?;
			c.write_u64::<LittleEndian>(meta.size)?;
		}

		Ok(data)
	}

}

pub struct NeedleFooter {
	
}

impl NeedleFooter {
	pub fn write(writer: &mut Write, sum: u32) -> Result<()> {
		writer.write_all(&FOOTER_MAGIC.as_bytes())?;
		writer.write_u32::<LittleEndian>(sum)?;
		Ok(())
	}

}


/// In memory representation of a single needle
/// Backed by a single buffer that is the entire size of the header, data and footer
pub struct Needle {
	pub header: NeedleHeader,
	buf: Vec<u8>
}

impl Needle {

	/// Reads a single needle at the current position in one read given known metadata for it
	pub fn read_oneshot(reader: &mut Read, meta: &NeedleMeta) -> Result<Needle> {
		let total_size = NEEDLE_HEADER_SIZE + (meta.size as usize) + NEEDLE_FOOTER_SIZE;

		let mut buf = Vec::new();
		buf.resize(total_size, 0u8); // TODO: Use an unsafe resize without filling

		reader.read_exact(&mut buf)?;

		let header = NeedleHeader::parse(array_ref!(&buf, 0, NEEDLE_HEADER_SIZE))?;
		
		let magic_start = NEEDLE_HEADER_SIZE + (header.meta.size as usize);
		if &buf[magic_start..(magic_start + FOOTER_MAGIC.len())] != FOOTER_MAGIC.as_bytes() {
			// Generally this means that it is legitamitely corrupt
			// Externalize as a corruption indicator
			return Err("Needle footer bad magic".into());
		}

		// Validate that the metadata we were given is actually correct
		if header.meta.size != meta.size {
			return Err("Inconsistently".into());
		}

		Ok(Needle {
			header,
			buf
		})
	}




	pub fn bytes(self) -> Bytes {
		Bytes::from(self.buf).slice(NEEDLE_HEADER_SIZE, self.header.meta.size as usize)
	}

	pub fn data(&self) -> &[u8] {
		&self.buf[NEEDLE_HEADER_SIZE..(NEEDLE_HEADER_SIZE + (self.header.meta.size as usize))]
	}

	pub fn crc32c(&self) -> &[u8] {
		let sum_start = NEEDLE_HEADER_SIZE + self.data().len() + HEADER_MAGIC_SIZE;
		(&self.buf[sum_start..])
	}

	/**
	 * Verifies the integrity of the needle's data based on the checksum
	 *
	 * If this gives an error, then this physical volume as at least partially corrupted
	 */
	pub fn check(&self) -> Result<()> {

		// TOOD: Insulate all errors so that we can distinguish corruption errors from other errors
			// TODO: Ideally this would all be effectively optional 

		let sum_expected = self.crc32c().read_u32::<LittleEndian>().unwrap();

		let sum = crc32c_append(0, self.data());

		if sum != sum_expected {
			// NOTE: I do want to support wrappning stuff
			return Err("Needle data does not match checksum".into());
		}

		Ok(())
	}

}