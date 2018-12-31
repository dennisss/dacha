extern crate rand;
extern crate arrayref;
extern crate futures;
extern crate bytes;

use std::io;
use std::io::{Write, Read, Seek};
use std::collections::HashMap;
use std::cmp::min;
use fs::{File, OpenOptions};
use crc32c::crc32c_append;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use rand::prelude::*;
use std::path::Path;
use arrayref::*;
use bytes::Bytes;

const SUPERBLOCK_SIZE: usize = 32;
const SUPERBLOCK_MAGIC: &str = "HAYS"; // And we will use HAYI for the index file
const FORMAT_VERSION: u32 = 1;
const COOKIE_SIZE: usize = 8;
const NEEDLE_ALIGNMENT: usize = 8;

const NEEDLE_HEADER_SIZE: usize = COOKIE_SIZE + 4 + 8 + 4 + 1 + 8; 
const NEEDLE_HEADER_MAGIC: &str = "NEED";
const NEEDLE_FOOTER_SIZE: usize = 4 + 4;
const NEEDLE_FOOTER_MAGIC: &str = "LES!";

const FLAG_DELETED: u8 = 1;


#[derive(Hash, Eq, PartialEq, Debug, Clone)]
pub struct HaystackNeedleKeys {
	pub key: u64,
	pub alt_key: u32
}

// but yes, we basically do need to 

#[derive(Clone)]
pub struct HaystackNeedleMeta {
	pub flags: u8,
	pub size: u64
}

pub struct HaystackNeedleIndexEntry  {
	pub meta: HaystackNeedleMeta,
	pub offset: u64
}


pub struct HaystackNeedleHeader {
	pub cookie: [u8; COOKIE_SIZE],
	pub keys: HaystackNeedleKeys,
	pub meta: HaystackNeedleMeta
}

impl HaystackNeedleHeader {
	fn parse(header: &[u8; NEEDLE_HEADER_SIZE]) -> io::Result<HaystackNeedleHeader> {

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
}

/*
struct PartialVec {
	buf: Vec<u8>,
	start: usize,
	end: usize,
	done: bool
}

impl<'a> futures::stream::Stream for PartialVec<'a> {
	type Item = &'a [u8];
	type Error = io::Error;

	fn poll(&'a mut self) -> futures::Poll<Option< &'a [u8] >, Self::Error> {
		if self.done {
			return Ok(futures::Async::Ready(None))
		}

		self.done = true;
		Ok(futures::Async::Ready(Some(
			&self.buf[self.start..self.end]
		)))
	}
}
*/


pub struct HaystackNeedle {
	pub header: HaystackNeedleHeader,
	buf: Vec<u8> // TODO: Will eventually need to become private again
}

impl HaystackNeedle {

	pub fn bytes(self) -> Bytes {
		Bytes::from(self.buf).slice(NEEDLE_HEADER_SIZE, self.header.meta.size as usize)
	}

	pub fn data(&self) -> &[u8] {
		&self.buf[NEEDLE_HEADER_SIZE..(NEEDLE_HEADER_SIZE + (self.header.meta.size as usize))]
	}

	pub fn stored_crc32c(&self) -> &[u8] {
		let sum_start = NEEDLE_HEADER_SIZE + self.data().len() + NEEDLE_HEADER_MAGIC.len();
		(&self.buf[sum_start..])
	}

	/**
	 * Verifies the integrity of the needle's data based on the checksum
	 *
	 * If this gives an error, then this physical volume as at least partially corrupted
	 */
	pub fn check(&self) -> io::Result<()> {
			// TODO: Ideally this would all be effectively optional 

		let sum_expected = self.stored_crc32c().read_u32::<LittleEndian>().unwrap();

		let sum = crc32c_append(0, self.data());

		if sum != sum_expected {
			return Err(io::Error::new(io::ErrorKind::Other, "Needle data does not match checksum"));
		}

		Ok(())
	}

}

// TODO: There is not really any point to indexing everything in memory if we will only ever use the offset (to read the )

// 
pub struct HaystackClusterConfig {

	/**
	 * Some universally uuid that identifies all physical and logical volumes within the current store/cluster
	 */
	pub cluster_id: [u8; 16],
	
	/**
	 * Location of all physical volume volumes
	 */
	pub volumes_dir: String
}


/**
 * Represents a single file on disk that consists of many photos as part of some logical volume
 */
pub struct HaystackPhysicalVolume {
	pub volume_id: u64,
	pub cluster_id: [u8; 16],
	file: File,

	// TODO: Make it a set of binary heaps so that we can efficiently look up all types for a single photo?
	index: HashMap<HaystackNeedleKeys, HaystackNeedleIndexEntry>
}

impl HaystackPhysicalVolume {

	// I need to know the: store directory, volume id, and the cluster_id to make this store

	pub fn create(config: &HaystackClusterConfig, volume_id: u64) -> io::Result<HaystackPhysicalVolume> {
		let mut opts = OpenOptions::new();
		opts.write(true).create_new(true).read(true);

		let path = Path::new(&config.volumes_dir).join(String::from("haystack_") + &volume_id.to_string());
		let f = opts.open(path)?;

		let mut vol = HaystackPhysicalVolume {
			volume_id,
			cluster_id: config.cluster_id,
			file: f,
			index: HashMap::new()
		};

		vol.write_superblock()?;

		Ok(vol)
	}
	
	/**
	 * Opens a volume given it's file name
	 */
	pub fn open(path: &str) -> io::Result<HaystackPhysicalVolume> {
		let mut opts = OpenOptions::new();
		opts.read(true).write(true);

		let mut f = opts.open(path)?;

		let mut vol = HaystackPhysicalVolume::read_superblock(&mut f)?;
		vol.scan_needles()?;

		Ok(vol)
	}

	// TODO: Would probably be nicer to have a separate superblock type to also 
	// TODO: Rather I should implement a more generic superblock reader for all types includin the index files
	fn read_superblock(file: &mut File) -> io::Result<HaystackPhysicalVolume>  {
		let mut header = [0u8; SUPERBLOCK_SIZE];
		file.read_exact(&mut header)?;

		if &header[0..4] != SUPERBLOCK_MAGIC.as_bytes() {
			return Err(io::Error::new(io::ErrorKind::Other, "Superblock magic is incorrect"));
		} 

		let ver = (&header[4..]).read_u32::<LittleEndian>()?;
		let volume_id = (&header[8..]).read_u64::<LittleEndian>()?;
		let cluster_id = &header[16..32];

		if ver != FORMAT_VERSION {
			return Err(io::Error::new(io::ErrorKind::Other, "Superblock unknown format version"));
		}

		let mut vol = HaystackPhysicalVolume {
			volume_id,
			cluster_id: [0u8; 16],
			file: file.try_clone()?,
			index: HashMap::new()
		};

		vol.cluster_id.copy_from_slice(cluster_id);

		Ok(vol)
	}

	/**
	 * Scans all of the needles in the file and builds the initial index from them
	 * (this should generally only be used if no separate index file is available)
	 */
	fn scan_needles(&mut self) -> io::Result<()> {

		let mut off = SUPERBLOCK_SIZE as u64;
		self.file.seek(io::SeekFrom::Start(SUPERBLOCK_SIZE as u64))?;

		let size = self.file.metadata()?.len();

		let mut buf = [0u8; NEEDLE_HEADER_SIZE];

		while off < size {
			println!("Reading needle at {}", off);

			self.file.read_exact(&mut buf)?;

			let n = HaystackNeedleHeader::parse(&buf)?;
			self.index.insert(n.keys.clone(), HaystackNeedleIndexEntry {
				meta: n.meta.clone(),
				offset: off
			});

			// Skip the body, footer, and padding
			off += (NEEDLE_HEADER_SIZE as u64) + n.meta.size + (NEEDLE_FOOTER_SIZE as u64);
			off += self.needle_pad_remaining(off);
			self.file.seek(io::SeekFrom::Start(off))?;
		}

		// TODO: Eventually if the file is incomplete, we should support partially rebuilding the needle starting at the end of all good looking data

		Ok(())
	}

	fn write_superblock(&mut self) -> io::Result<()> {
		
		self.file.write_all(SUPERBLOCK_MAGIC.as_bytes())?;
		self.file.write_u32::<LittleEndian>(FORMAT_VERSION)?;
		self.file.write_u64::<LittleEndian>(self.volume_id)?;
		self.file.write_all(&self.cluster_id)?;

		Ok(())
	}

	// TODO: If we want to go super fast, we could implement the data as a stream and start sending it back to a user right away
	/**
	 * Tries to read a single needle from the volume
	 * Will only return if it exists, has not been deleted
	 *
	 * NOTE: The needle still needs to be separately checked for integrity
	 */
	pub fn read_needle(&mut self, keys: &HaystackNeedleKeys) -> io::Result<Option<HaystackNeedle>> {

		let mut entry = match self.index.get_mut(keys) {
			Some(e) => e,
			None => return Ok(None)
		};

		// Do not return deleted files
		if entry.meta.flags & FLAG_DELETED != 0 {
			return Ok(None);
		}

		let total_size = NEEDLE_HEADER_SIZE + (entry.meta.size as usize) + NEEDLE_FOOTER_SIZE;

		let mut buf = Vec::new();
		buf.resize(total_size, 0u8); // TODO: Use an unsafe resize without filling

		self.file.seek(io::SeekFrom::Start(entry.offset))?;
		self.file.read_exact(&mut buf)?;

		let header = HaystackNeedleHeader::parse(array_ref!(&buf, 0, NEEDLE_HEADER_SIZE))?;
		
		// Update index with most up-to-date flags
		entry.meta.flags = header.meta.flags;

		// Separate index files do not persist deletes, so we will be double check the main flags 
		if header.meta.flags & FLAG_DELETED != 0 {
			return Ok(None);
		}

		// Validate that the index is still consistent with the main file
		if header.meta.size != entry.meta.size {
			return Err(io::Error::new(io::ErrorKind::Other, "Inconsistently"));
		}

		let magic_start = NEEDLE_HEADER_SIZE + (header.meta.size as usize);
		if &buf[magic_start..(magic_start + NEEDLE_FOOTER_MAGIC.len())] != NEEDLE_FOOTER_MAGIC.as_bytes() {
			return Err(io::Error::new(io::ErrorKind::Other, "Needle footer bad magic"));
		}


		Ok(Some(HaystackNeedle {
			header,
			buf
		}))
	}

	// TODO: We will likely also want to have a create operation that gurantees that a needle does not exist
	/**
	 * Adds a new needle to the very end of the file (overriding any previous needle for the same keys)
	 *
	 * TODO: Probably most useful to return a reference to the full needle entry 
	 */
	pub fn append_needle(&mut self, keys: &HaystackNeedleKeys, meta: &HaystackNeedleMeta, data: &mut Read) -> io::Result<()> {
		let mut rng = rand::thread_rng();
		let mut buf = [0u8; 8*1024 /* io::DEFAULT_BUF_SIZE */];

		// Write at the end of the file (and get that offset)
		let off = self.file.seek(io::SeekFrom::End(0))?;

		let mut cookie = Vec::new();
		cookie.resize(COOKIE_SIZE, 0u8);
		rng.fill_bytes(&mut cookie);


		let mut header = Vec::new();
		header.write_all(NEEDLE_HEADER_MAGIC.as_bytes())?;
		header.write_all(&cookie)?;
		header.write_u64::<LittleEndian>(keys.key)?;
		header.write_u32::<LittleEndian>(keys.alt_key)?;
		header.write_u8(meta.flags)?;
		header.write_u64::<LittleEndian>(meta.size)?;
		self.file.write_all(&header)?;


		let mut sum = 0;

		// Pretty much io::copy but with the output split into making the hash and writing to the output
		//io::copy(&mut data, &mut self.file)?;
		let mut nread: usize = 0;
		while nread < (meta.size as usize) {
			let left = min(buf.len(), (meta.size as usize) - nread);
			let n = data.read(&mut buf[0..left])?;
			if n == 0 {
				// End of file/stream
				break;
			}

			let chunk = &buf[..n];
			sum = crc32c_append(sum, chunk);
			self.file.write_all(chunk)?;

			nread += n;
		}

		if nread != (meta.size as usize) {
			// Big error: we did read enough bytes
			// TODO: Another consideration is that for a stream that is a single file, it needs to be at the end of the file now
		}


		self.file.write_all(&NEEDLE_FOOTER_MAGIC.as_bytes())?;

		self.file.write_u32::<LittleEndian>(sum)?;

		let pos = self.file.seek(io::SeekFrom::Current(0))?;
		let pad = self.needle_pad_remaining(pos);
		if pad != 0 {
			let mut padding = Vec::new();
			padding.resize(pad as usize, 0);
			self.file.write_all(&padding)?;
		}

		self.index.insert(keys.clone(), HaystackNeedleIndexEntry {
			meta: meta.clone(),
			offset: off
		});

		Ok(())
	}

	pub fn delete_needle(&mut self, keys: &HaystackNeedleKeys) -> io::Result<()> {

		let entry = match self.index.get(keys) {
			Some(e) => e,
			None => return Err(io::Error::new(io::ErrorKind::Other, "Needle does not exist")),
		};

		if entry.meta.flags & FLAG_DELETED != 0 {
			return Err(io::Error::new(io::ErrorKind::Other, "Needle already deleted"));
		}

		// read the header in the file

		// double check it's flag isn't already set

		// write back to the file in place

		Ok(())
	}


	/**
	 * Given that the current position in the file is at the end of a middle, this will determine how much 
	 */
	fn needle_pad_remaining(&mut self, end_offset: u64) -> u64 {
		let rem = (end_offset as usize) % NEEDLE_ALIGNMENT;
		if rem == 0 {
			return 0;
		}

		(NEEDLE_ALIGNMENT - rem) as u64
	}

}

