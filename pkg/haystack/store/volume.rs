
use super::super::common::*;
use super::super::errors::*;
use super::super::paths::CookieBuf;
use super::needle::*;
use std::io;
use std::io::{Write, Read, Seek, Cursor};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use crc32c::crc32c_append;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use std::path::Path;
use std::mem::size_of;
use super::stream::Stream;

const SUPERBLOCK_MAGIC: &str = "HAYS"; // And we will use HAYI for the index file
const SUPERBLOCK_MAGIC_SIZE: usize = 4;

const SUPERBLOCK_SIZE: usize =
	SUPERBLOCK_MAGIC_SIZE +
	size_of::<FormatVersion>() +
	size_of::<ClusterId>() +
	size_of::<MachineId>() +
	size_of::<VolumeId>();


/// Simple wrapper around a read needle including the offset into the file (useful for etags)
pub struct NeedleWithOffset {
	pub block_offset: u32,
	pub needle: Needle
}

// TODO: We'd also like to be able to set an entire physical volume as write_enabled
// - Mainly useful so that we can report it back to clients and so that next time we need to broadcast that we are out of space, we only need to mark volumes which we haven't yet marked as disabled

/// Represents a single file on disk that consists of many photos as part of some logical volume
pub struct PhysicalVolume {
	pub cluster_id: ClusterId,
	pub machine_id: MachineId,
	pub volume_id: VolumeId,
	file: File,

	// TODO: Make it a set of binary heaps so that we can efficiently look up all types for a single photo?
	index: HashMap<NeedleKeys, NeedleIndexEntry>,

	/// Number of bytes that we estimate can be gained through compaction
	compaction_pending: u64,

	/// The lowest needle offset in the file that will require compaction (or 0 if we've never compacted before)
	compaction_watermark: u64,

	/// End offset of the last needle that all of the other compaction information represents
	//compaction_checkpoint: u64,

	/// The length of the file (or the offset to the very end of the last needle + padding)
	/// Because of the potential for partial writes, we won't trust the size reported on disk after the volume is fully loaded
	extent: u64
}

impl PhysicalVolume {

	// I need to know the: store directory, volume id, and the cluster_id to make this store

	/// Creates a new empty volume and corresponding index file
	/// 
	/// Will error out if the volume already exists
	pub fn create(
		path: &Path, cluster_id: ClusterId, machine_id: MachineId, volume_id: VolumeId
	) -> Result<PhysicalVolume> {
		
		let mut opts = OpenOptions::new();
		opts.write(true).create_new(true).read(true);

		let f = opts.open(path)?;

		let mut vol = PhysicalVolume {
			machine_id,
			volume_id,
			cluster_id,
			file: f,
			index: HashMap::new(),
			compaction_pending: 0,
			compaction_watermark: 0,
			extent: 0
		};

		vol.write_superblock()?;
		vol.extent = vol.offset_after_super_block();

		// Then we will initialize an empty index file
		// In the case of 

		Ok(vol)
	}
	
	// Likely also to be based on the same params
	/// Opens a volume given it's file name
	///
	///  XXX: Ideally we would have some better way of doing this right?
	pub fn open(path: &Path) -> Result<PhysicalVolume> {
		let mut opts = OpenOptions::new();
		opts.read(true).write(true);

		let mut f = opts.open(path)?;

		let mut vol = PhysicalVolume::read_superblock(&mut f)?;
		vol.scan_needles()?;

		Ok(vol)
	}

	// TODO: Would probably be nicer to have a separate superblock type to also 
	// TODO: Rather I should implement a more generic superblock reader for all types includin the index files
	fn read_superblock(file: &mut File) -> Result<PhysicalVolume>  {
		let mut header = [0u8; SUPERBLOCK_SIZE];
		file.read_exact(&mut header)?;

		if &header[0..4] != SUPERBLOCK_MAGIC.as_bytes() {
			return Err("Superblock magic is incorrect".into());
		}

		let mut c = Cursor::new(&header[4..]);
		let ver = c.read_u32::<LittleEndian>()?;
		let cluster_id = c.read_u64::<LittleEndian>()?;
		let machine_id = c.read_u32::<LittleEndian>()?;
		let volume_id = c.read_u32::<LittleEndian>()?;

		if ver != CURRENT_FORMAT_VERSION {
			return Err("Superblock unknown format version".into());
		}

		let mut vol = PhysicalVolume {
			cluster_id,
			machine_id,
			volume_id,
			file: file.try_clone()?,
			index: HashMap::new(),
			compaction_pending: 0,
			compaction_watermark: 0,
			extent: 0
		};

		// Initially starts right after the superblock because we haven't checked any of the needles after it yet
		vol.extent = vol.offset_after_super_block();

		Ok(vol)
	}

	/// Gets the number of raw needles stored 
	pub fn num_needles(&self) -> usize {
		self.index.len()
	}

	// Lists the size of all space currently being used by this volume and any associated index
	pub fn used_space(&self) -> usize {
		self.file.metadata().unwrap().len() as usize
	}


	/// Scans all of the needles in the file and builds the initial index from them
	/// 
	/// (this should generally only be used if no separate index file is available)
	/// 
	/// TODO: We should also use this for checking the integrity of an existing file
	fn scan_needles(&mut self) -> Result<()> {

		// Start scanning at last known good end of file
		let mut off = self.extent;

		self.file.seek(io::SeekFrom::Start(off))?;

		let size = self.file.metadata()?.len();

		let mut buf = [0u8; NEEDLE_HEADER_SIZE];
		let mut last_off = 0;

		while off < size {

			last_off = off;

			if off % (BLOCK_SIZE as u64) != 0 {
				return Err("Needles misaligned relative to block offsets".into());
			}
			
			let block_offset = (off / (BLOCK_SIZE as u64)) as BlockOffset;

			println!("Reading needle at {}", off);

			self.file.read_exact(&mut buf)?;

			let n = NeedleHeader::parse(&buf)?;
			self.index.insert(n.keys.clone(), NeedleIndexEntry {
				meta: n.meta.clone(),
				block_offset
			});

			// Skip the body, footer, and padding
			off += (NEEDLE_HEADER_SIZE as u64) + n.meta.size + (NEEDLE_FOOTER_SIZE as u64);
			off += self.block_size_remainder(off);
			self.file.seek(io::SeekFrom::Start(off))?;
		}

		if size == off {
			// Perform file
			self.extent = off;
		}
		else {
			eprintln!("Detected incomplete data at end of file");

			// Truncating to the end of the last file (we will just overwrite the existing data when we start appending more data)
			self.extent = last_off;
		}

		Ok(())
	}

	fn write_superblock(&mut self) -> Result<()> {
		
		self.file.seek(io::SeekFrom::Start(0))?;
		self.file.write_all(SUPERBLOCK_MAGIC.as_bytes())?;
		self.file.write_u32::<LittleEndian>(CURRENT_FORMAT_VERSION)?;
		self.file.write_u64::<LittleEndian>(self.cluster_id)?;
		self.file.write_u32::<LittleEndian>(self.machine_id)?;
		self.file.write_u32::<LittleEndian>(self.volume_id)?;
		// TODO: Write the checksum of all of this stuff (minus the padding)

		let end = self.pad_to_block_size()?;
		self.file.flush()?;

		if self.extent == 0 {
			self.extent = end;
		}

		Ok(())
	}

	/// See what the offset of a needle is as fast as possible (mainly a cache optimization for etags received upstream from the cache)
	pub fn peek_needle_block_offset(&self, keys: &NeedleKeys) -> Option<BlockOffset> {
		self.index.get(keys).map(|e| e.block_offset)
	}

	// TODO: If we want to go super fast, we could implement the data as a stream and start sending it back to a user right away
	/**
	 * Tries to read a single needle from the volume
	 * Will only return if it exists, has not been deleted
	 *
	 * NOTE: The needle still needs to be separately checked for integrity
	 */
	pub fn read_needle(&mut self, keys: &NeedleKeys) -> Result<Option<NeedleWithOffset>> {

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

		self.file.seek(io::SeekFrom::Start(entry.offset()))?;

		let needle = Needle::read_oneshot(&mut self.file, &entry.meta)?;

		// Update index with most up-to-date flags
		entry.meta.flags = needle.header.meta.flags;

		// Separate index files do not persist deletes, so we will be double check the main flags 
		if needle.header.meta.deleted() {
			return Ok(None);
		}
		
		Ok(Some(NeedleWithOffset {
			needle,
			block_offset: entry.block_offset
		}))
	}

	// TODO: We will likely also want to have a create operation that gurantees that a needle does not exist
	/// Adds a new needle to the very end of the file (overriding any previous needle for the same keys)
	/// 
	/// TODO: Probably most useful to return a reference to the full needle entry
	pub fn append_needle(
		// In almost all cases, we can defer the chunking decision 
		&mut self, keys: NeedleKeys, cookie: CookieBuf, meta: NeedleMeta, data: &mut Stream
	) -> Result<()> {

		// Typically needles will not be overwritten, but if they are, we consider needles with the same exact keys/cookie to be identical, so we will ignore attempts to update them
		// TODO: The main exception to this will be error correction (in which case we to be able to do this)
		if let Some(existing) = self.index.get(&keys) {
			
			// TODO: We can no longer do deduplication of uploads

			// TODO: This now incentivizes making sure that parsing of needle headers is efficient and doesn't do as many copies
			//self.file.seek(io::SeekFrom::Start(existing.offset()))?;
			//let existing_header = NeedleHeader::read(&mut self.file)?;

			//if cookie == &existing_header.cookie {
			//	println!("Ignoring request to upload exact same needle twice");
			//	return Ok(());
			//}
		}
		

		let mut buf = [0u8; 8*1024 /* io::DEFAULT_BUF_SIZE */];

		// Seek to the end of the file (and get that offset)
		// TODO: Instead we should be tracking the end as the offset after the last known good needle (as we don't want to compound corruptions)
		let off = self.extent;
		self.file.seek(io::SeekFrom::Start(off))?;

		if off % (BLOCK_SIZE as u64) != 0 {
			return Err("File not block aligned".into());
		}

		let block_offset = (off / (BLOCK_SIZE as u64)) as BlockOffset;


		let header: Vec<u8> = NeedleHeader::serialize(cookie.data(), &keys, &meta)?;
		self.file.write_all(&header)?;


		let mut sum = 0;

		// Pretty much io::copy but with the output split into making the hash and writing to the output
		//io::copy(&mut data, &mut self.file)?;
		let mut nread: usize = 0;
		while nread < (meta.size as usize) {

			let left = (meta.size as usize) - nread;

			// TODO: If we get a source error, we should probably still truncate our store file to avoid further corruption
			let chunk = match data.next(left)? {
				Some(c) => c,
				None => break
			};

			nread += chunk.len();

			// Don't even bother writing it if it would take us over
			if nread > (meta.size as usize) {
				break;
			}

			sum = crc32c_append(sum, chunk);
			self.file.write_all(chunk)?;
		}

		if nread != (meta.size as usize) {
			// Big error: we did read enough bytes
			// TODO: Another consideration is that for a stream that is a single file, it needs to be at the end of the file now

			self.file.set_len(off)?;

			return Err("Not enough bytes could be read".into());
		}


		// TODO: These two writes can definitely be combined
		NeedleFooter::write(&mut self.file, sum)?;
		
		// Pad the file to the blocksize and mark our new file length
		self.extent = self.pad_to_block_size()?;

		self.index.insert(keys.clone(), NeedleIndexEntry {
			meta: meta.clone(),
			block_offset
		});

		Ok(())
	}

	pub fn delete_needle(&mut self, keys: &NeedleKeys) -> Result<()> {

		let entry = match self.index.get(keys) {
			Some(e) => e,
			None => return Err("Needle does not exist".into()),
		};

		if entry.meta.deleted() {
			return Err("Needle already deleted".into());
		}

		//entry.offset.

		// read the header in the file

		// double check it's flag isn't already set

		// write back to the file in place

		Ok(())
	}


	fn pad_to_block_size(&mut self) -> Result<u64> {
		let pos = self.file.seek(io::SeekFrom::Current(0))?;
		let pad = self.block_size_remainder(pos);
		if pad != 0 {
			let mut padding = Vec::new();
			padding.resize(pad as usize, 0);
			self.file.write_all(&padding)?;
		}

		Ok(pos + pad)
	}

	fn offset_after_super_block(&self) -> u64 {
		let mut off = SUPERBLOCK_SIZE as u64;
		off += self.block_size_remainder(off);
		off
	}

	/// Given that the current position in the file is at the end of a middle, this will determine how much 
	fn block_size_remainder(&self, end_offset: u64) -> u64 {
		let rem = (end_offset as usize) % BLOCK_SIZE;
		if rem == 0 {
			return 0;
		}

		(BLOCK_SIZE - rem) as u64
	}

}

