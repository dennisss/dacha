use std::io;
use std::io::Cursor;
use std::io::{Write, Read, Seek};
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use bytes::Bytes;
use crc32c::crc32c_append;
use arrayref::*;
use super::common::*;

// TODO: Eventually make all of these private again
pub const NEEDLE_ALIGNMENT: usize = 8;
pub const NEEDLE_HEADER_SIZE: usize = COOKIE_SIZE + 4 + 8 + 4 + 1 + 8; 
pub const NEEDLE_HEADER_MAGIC: &str = "NEED";
pub const NEEDLE_FOOTER_SIZE: usize = 4 + 4;
pub const NEEDLE_FOOTER_MAGIC: &str = "LES!";

const FLAG_DELETED: u8 = 1;



#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct HaystackNeedleKeys {
	pub key: u64,
	pub alt_key: u32
}

#[derive(Clone)]
pub struct HaystackNeedleMeta {
	pub flags: u8,
	pub size: u64
}

impl HaystackNeedleMeta {
	pub fn deleted(&self) -> bool {
		self.flags & FLAG_DELETED != 0
	}
}

// Basically all of this can be 
pub struct HaystackNeedleIndexEntry  {
	pub meta: HaystackNeedleMeta,
	pub offset: u64
}


pub struct HaystackNeedleHeader {
	pub cookie: Cookie,
	pub keys: HaystackNeedleKeys,
	pub meta: HaystackNeedleMeta
}


impl HaystackNeedleHeader {

	pub fn read(reader: &mut Read) -> io::Result<HaystackNeedleHeader> {
		let mut buf = [0u8; NEEDLE_HEADER_SIZE];
		reader.read_exact(&mut buf)?;
		HaystackNeedleHeader::parse(&buf)
	}

	pub fn parse(header: &[u8; NEEDLE_HEADER_SIZE]) -> io::Result<HaystackNeedleHeader> {

		if &header[0..4] != NEEDLE_HEADER_MAGIC.as_bytes() {
			return Err(io::Error::new(io::ErrorKind::Other, "Needle header magic is incorrect"));
		}

		let mut pos = 4;
		let cookie = &header[pos..(pos + COOKIE_SIZE)]; pos += COOKIE_SIZE;
		let key = (&header[pos..]).read_u64::<LittleEndian>()?; pos += 8;
		let alt_key = (&header[pos..]).read_u32::<LittleEndian>()?; pos += 4;
		let flags = header[pos]; pos += 1;
		let size =  (&header[pos..]).read_u64::<LittleEndian>()?; pos += 8;

		// Ideally to be implemented in terms of 

		let mut n = HaystackNeedleHeader {
			cookie: [0u8; COOKIE_SIZE],
			keys: HaystackNeedleKeys {
				key, alt_key
			},
			meta: HaystackNeedleMeta {
				flags,
				size
			}
		};

		n.cookie.copy_from_slice(cookie);

		Ok(n)
	}

	// Annoyingly this will require a copy to do this 
	pub fn serialize(cookie: &Cookie, keys: &HaystackNeedleKeys, meta: &HaystackNeedleMeta) -> io::Result<Vec<u8>> {
		let mut data = Vec::new();
		data.reserve(NEEDLE_HEADER_SIZE);

		{
			let mut c = Cursor::new(&mut data);
			c.write_all(NEEDLE_HEADER_MAGIC.as_bytes())?;
			c.write_all(cookie)?;
			c.write_u64::<LittleEndian>(keys.key)?;
			c.write_u32::<LittleEndian>(keys.alt_key)?;
			c.write_u8(meta.flags)?;
			c.write_u64::<LittleEndian>(meta.size)?;
		}

		Ok(data)
	}

}

pub struct HaystackNeedle {
	pub header: HaystackNeedleHeader,
	buf: Vec<u8>
}

impl HaystackNeedle {

	/// Reads a single needle at the current position in one read given known metadata for it
	pub fn read_oneshot(reader: &mut Read, meta: &HaystackNeedleMeta) -> io::Result<HaystackNeedle> {
		let total_size = NEEDLE_HEADER_SIZE + (meta.size as usize) + NEEDLE_FOOTER_SIZE;

		let mut buf = Vec::new();
		buf.resize(total_size, 0u8); // TODO: Use an unsafe resize without filling

		reader.read_exact(&mut buf)?;

		let header = HaystackNeedleHeader::parse(array_ref!(&buf, 0, NEEDLE_HEADER_SIZE))?;
		
		let magic_start = NEEDLE_HEADER_SIZE + (header.meta.size as usize);
		if &buf[magic_start..(magic_start + NEEDLE_FOOTER_MAGIC.len())] != NEEDLE_FOOTER_MAGIC.as_bytes() {
			// Generally this means that it is legitamitely corrupt
			// Externalize as a corruption indicator
			return Err(io::Error::new(io::ErrorKind::Other, "Needle footer bad magic"));
		}

		// Validate that the metadata we were given is actually correct
		if header.meta.size != meta.size {
			return Err(io::Error::new(io::ErrorKind::Other, "Inconsistently"));
		}

		Ok(HaystackNeedle {
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
		let sum_start = NEEDLE_HEADER_SIZE + self.data().len() + NEEDLE_HEADER_MAGIC.len();
		(&self.buf[sum_start..])
	}

	/**
	 * Verifies the integrity of the needle's data based on the checksum
	 *
	 * If this gives an error, then this physical volume as at least partially corrupted
	 */
	pub fn check(&self) -> io::Result<()> {

		// TOOD: Insulate all errors so that we can distinguish corruption errors from other errors
			// TODO: Ideally this would all be effectively optional 

		let sum_expected = self.crc32c().read_u32::<LittleEndian>().unwrap();

		let sum = crc32c_append(0, self.data());

		if sum != sum_expected {
			// NOTE: I do want to support wrappning stuff
			return Err(io::Error::new(io::ErrorKind::Other, "Needle data does not match checksum"));
		}

		Ok(())
	}

}