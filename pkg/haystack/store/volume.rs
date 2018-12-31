extern crate rand;
extern crate arrayref;
extern crate futures;
extern crate bytes;

use super::needle::*;
use super::common::*;
use std::io;
use std::io::{Write, Read, Seek};
use std::collections::HashMap;
use std::cmp::min;
use fs::{File, OpenOptions};
use crc32c::crc32c_append;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use std::path::Path;


const SUPERBLOCK_SIZE: usize = 32;
const SUPERBLOCK_MAGIC: &str = "HAYS"; // And we will use HAYI for the index file
const FORMAT_VERSION: u32 = 1;



// Opening a physical volume will likewise required


/**
 * Represents a single file on disk that consists of many photos as part of some logical volume
 */
pub struct HaystackPhysicalVolume {
	pub volume_id: u64,
	pub cluster_id: ClusterId,
	file: File,

	// TODO: Make it a set of binary heaps so that we can efficiently look up all types for a single photo?
	index: HashMap<HaystackNeedleKeys, HaystackNeedleIndexEntry>
}

impl HaystackPhysicalVolume {

	// I need to know the: store directory, volume id, and the cluster_id to make this store

	// Basically I need a cluster_id, and a path for where to put it
	pub fn create(path: &Path, cluster_id: &ClusterId, volume_id: u64) -> io::Result<HaystackPhysicalVolume> {
		let mut opts = OpenOptions::new();
		opts.write(true).create_new(true).read(true);

		let f = opts.open(path)?;

		let mut vol = HaystackPhysicalVolume {
			volume_id,
			cluster_id: cluster_id.clone(),
			file: f,
			index: HashMap::new()
		};

		vol.write_superblock()?;

		// Then we will initialize an empty index file
		// In the case of 

		Ok(vol)
	}
	
	// Likely also to be based on the same params
	/// Opens a volume given it's file name
	///
	///  XXX: Ideally we would have some better way of doing this right?
	pub fn open(path: &Path) -> io::Result<HaystackPhysicalVolume> {
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

	/// Gets the number of raw needles stored 
	pub fn len_needles(&self) -> usize {
		self.index.len()
	}

	// TODO: We want to track stats on:
	// - total bytes size of the volume file
	// - total bytes size of the volume's index file
	// - combined size of all needle data segements in this volume 

	/// Gets the number of bytes on disk occupied by this volume (excluding the separate index)
	//pub fn len_bytes(&mut self) -> usize {
	//	self.file.seek(io::SeekFrom::End(0))? as usize
	//}


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
		self.file.flush()?;

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

		// This is basically the matter of reading from the current position in the thing

		// TODO: We go need to distinguish this case as being totally different
		let entry = match self.index.get_mut(keys) {
			Some(e) => e,
			None => return Ok(None)
		};

		// Do not return deleted files
		if entry.meta.deleted() {
			return Ok(None);
		}

		self.file.seek(io::SeekFrom::Start(entry.offset))?;

		let needle = HaystackNeedle::read_oneshot(&mut self.file, &entry.meta)?;

		// Update index with most up-to-date flags
		entry.meta.flags = needle.header.meta.flags;

		// Separate index files do not persist deletes, so we will be double check the main flags 
		if needle.header.meta.deleted() {
			return Ok(None);
		}
		
		Ok(Some(needle))
	}

	// TODO: We will likely also want to have a create operation that gurantees that a needle does not exist
	/// Adds a new needle to the very end of the file (overriding any previous needle for the same keys)
	/// 
	/// TODO: Probably most useful to return a reference to the full needle entry
	pub fn append_needle(&mut self, keys: &HaystackNeedleKeys, cookie: &Cookie, meta: &HaystackNeedleMeta, data: &mut Read) -> io::Result<()> {

		// Typically needles will not be overwritten, but if they are, we consider needles with the same exact keys/cookie to be identical, so we will ignore attempts to update them
		// TODO: The main exception to this will be error correction (in which case we to be able to do this)
		if let Some(existing) = self.index.get(&keys) {
			
			// TODO: This now incentivizes making sure that parsing of needle headers is efficient and doesn't do as many copies
			self.file.seek(io::SeekFrom::Start(existing.offset))?;
			let existing_header = HaystackNeedleHeader::read(&mut self.file)?;

			if cookie == &existing_header.cookie {
				println!("Ignoring request to upload exact same needle twice");
				return Ok(());
			}
		}
		

		let mut buf = [0u8; 8*1024 /* io::DEFAULT_BUF_SIZE */];

		// Write at the end of the file (and get that offset)
		let off = self.file.seek(io::SeekFrom::End(0))?;

		let header = HaystackNeedleHeader::serialize(&cookie, keys, meta)?;
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

			self.file.set_len(off)?;

			return Err(io::Error::new(io::ErrorKind::Other, "Not enough bytes could be read"));
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

		if entry.meta.deleted() {
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

