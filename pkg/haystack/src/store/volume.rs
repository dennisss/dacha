
use super::super::common::*;
use super::super::errors::*;
use super::api::CookieBuf;
use super::needle::*;
use super::volume_index::*;
use super::superblock::*;
use std::io;
use std::io::{Write, Read, Seek, Cursor};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use crc32c::crc32c_append;
use std::path::{Path, PathBuf};
use super::stream::Stream;
use core::block_size_remainder;
use fs2::FileExt;
use core::allocate_soft::*;

const SUPERBLOCK_MAGIC: &str = "HAYS";



/// Simple wrapper around a read needle including the offset into the file (useful for etags)
pub struct NeedleWithOffset {
	pub block_offset: u32,
	pub needle: Needle
}

// TODO: We'd also like to be able to set an entire physical volume as write_enabled
// - Mainly useful so that we can report it back to clients and so that next time we need to broadcast that we are out of space, we only need to mark volumes which we haven't yet marked as disabled

/// Represents a single file on disk that consists of many photos as part of some logical volume
pub struct PhysicalVolume {
	pub superblock: PhysicalVolumeSuperblock,
	config: ConfigRef,
	path: PathBuf,
	file: File,

	// TODO: Make it a set of binary heaps so that we can efficiently look up all types for a single photo?
	index: HashMap<NeedleKeys, NeedleIndexEntry>,
	index_file: PhysicalVolumeIndex,

	/// Number of bytes that we estimate can be gained through compaction
	compaction_pending: u64,

	/// Some physical 
	//compaction_active: Option<Arc<PhysicalVolume>>,

	/// The length of the file (or the offset to the very end of the last needle + padding)
	/// Because of the potential for partial writes, we won't trust the size reported on disk after the volume is fully loaded
	extent: u64,

	/// Amount of space allocated in the FS for this file (should be >= the current size of the file)
	preallocated: u64
}

impl PhysicalVolume {

	// I need to know the: store directory, volume id, and the cluster_id to make this store

	/// Creates a new empty volume and corresponding index file
	/// 
	/// Will error out if the volume already exists
	pub fn create(
		config: ConfigRef, path: &Path, cluster_id: ClusterId, machine_id: MachineId, volume_id: VolumeId
	) -> Result<PhysicalVolume> {
		
		let mut opts = OpenOptions::new();
		opts.write(true).create_new(true).read(true);

		let file = opts.open(path)?;

		// Sync directory
		File::open(path.parent().unwrap()).unwrap().sync_all()?;

		let superblock = PhysicalVolumeSuperblock {
			magic: SUPERBLOCK_MAGIC.as_bytes().into(),
			cluster_id,
			machine_id,
			volume_id,
			block_size: config.store.block_size,
			allocated_space: config.store.allocation_size
		};

		let idx_path = path.to_str().unwrap().to_owned() + ".idx";
		let idx = PhysicalVolumeIndex::create(&Path::new(&idx_path), &superblock)?;

		let preallocated = file.allocated_size()?;

		let mut vol = PhysicalVolume {
			superblock,
			config,
			path: path.to_owned(),
			file,
			index: HashMap::new(),
			index_file: idx,
			compaction_pending: 0,
			extent: 0,
			preallocated
		};

		vol.superblock.write(&mut vol.file)?;

		let end = vol.pad_to_block_size()?;
		vol.file.sync_data()?;
		vol.extent = end;

		Ok(vol)
	}
	
	// Likely also to be based on the same params
	/// Opens a volume given it's file name
	///
	///  XXX: Ideally we would have some better way of doing this right?
	pub fn open(config: ConfigRef, path: &Path) -> Result<PhysicalVolume> {
		let mut opts = OpenOptions::new();
		opts.read(true).write(true);

		let mut file = opts.open(path)?;

		let superblock = PhysicalVolumeSuperblock::read(&mut file)?;

		if &superblock.magic[..] != SUPERBLOCK_MAGIC.as_bytes() {
			return Err("Superblock magic is incorrect".into());
		}

		let idx_path_string = path.to_str().unwrap().to_owned() + ".idx";
		let idx_path = Path::new(&idx_path_string);
		let idx = if idx_path.exists() {
			// TODO: In most cases of failures to read existing indexes, we can just toss it out and regenerate a new one

			let i = PhysicalVolumeIndex::open(&idx_path)?;

			if i.superblock.cluster_id != superblock.cluster_id || i.superblock.machine_id != superblock.machine_id || i.superblock.volume_id != superblock.volume_id {
				return Err("Opened an index file for a mismatching volume".into())
			}

			i
		}
		else {
			// Read just the superblock
			PhysicalVolumeIndex::create(&idx_path, &superblock)?
		};

		let preallocated = file.allocated_size()?;

		let mut vol = PhysicalVolume {
			superblock,
			config,
			path: path.to_owned(),
			file,
			index: HashMap::new(),
			index_file: idx,
			compaction_pending: 0,
			extent: 0,
			preallocated
		};

		// Initially starts right after the superblock because we haven't checked any of the needles after it yet
		vol.extent = vol.offset_after_super_block();

		vol.scan_needles()?;

		Ok(vol)
	}


	pub fn can_write_soft(&self) -> bool {
		(((self.used_space() as f64) * 0.95) as u64) < self.superblock.allocated_space
	}

	pub fn can_write(&self) -> bool {
		self.used_space() < self.superblock.allocated_space
	}


	/// Gets the number of raw needles stored 
	pub fn num_needles(&self) -> usize {
		self.index.len()
	}

	/// Lists the size of all space currently being used by this volume and any associated index
	/// This will essentially be the total storage cost of this volume not containing lower-level filesystem metadata
	pub fn used_space(&self) -> u64 {
		// TODO: May be slightly off as we don't immediately truncate the file after failed writes or extra data at the end of it (as we'd rather try to avoid truncatating pre-emptively in-case a human wants to )
		self.extent + self.index_file.used_space()
	}

	/// Internal utility for adding to the index
	/// This should be atomic w.r.t in-memory datastructures as long as it doesn't panic
	fn add_to_index(&mut self, keys: NeedleKeys, entry: NeedleIndexEntry, from_index_file: bool) -> Result<()> {
		
		if !from_index_file {
			self.index_file.append(&keys, &entry)?;
		}

		if let Some(old_val) = self.index.get(&keys) {
			if old_val.block_offset == entry.block_offset {
				// This isn't really problematic, but does indicate that we are doing something wrong
				return Err("Adding the exact same index entry twice")?;
			}

			self.compaction_pending += old_val.meta.occupied_size(self.superblock.block_size)
		}

		self.index.insert(keys, entry);

		Ok(())
	}

	/// Scans all of the needles in the file and builds the initial index from them
	/// 
	/// (this should generally only be used if no separate index file is available)
	/// 
	/// TODO: We should also use this for checking the integrity of an existing file
	fn scan_needles(&mut self) -> Result<()> {

		// Start scanning at last known good end of file
		let mut off = self.extent;

		// Start by taking all entries from the condensed index file and seeking to the end of those
		let max_extent = self.file.metadata().unwrap().len();
		let index_pairs = self.index_file.read_all(max_extent)?;

		if index_pairs.len() > 0 {
			{
				let p = &index_pairs[index_pairs.len() - 1];
				off = p.value.end_offset(self.superblock.block_size);
			}

			for pair in index_pairs {
				// TODO: This will end up readding it the 
				self.add_to_index(pair.keys, pair.value, true)?;
			}
		}


		self.file.seek(io::SeekFrom::Start(off))?;

		let size = self.file.metadata()?.len();

		let mut buf = [0u8; NEEDLE_HEADER_SIZE];
		let mut last_off = off;

		// Reading all remaining orphans in the file
		while off + (NEEDLE_HEADER_SIZE as u64) <= size {

			last_off = off;

			if off % self.superblock.block_size != 0 {
				return Err("Needles misaligned relative to block offsets".into());
			}
			
			let block_offset = (off / self.superblock.block_size) as BlockOffset;

			println!("Reading needle at {}", off);

			self.file.read_exact(&mut buf)?;

			let n = NeedleHeader::parse(&buf)?;

			let entry = NeedleIndexEntry {
				meta: n.meta.clone(),
				block_offset
			};

			off = entry.end_offset(self.superblock.block_size);

			self.add_to_index(n.keys.clone(), entry, false)?;

			self.file.seek(io::SeekFrom::Start(off))?;
		}

		if size == off {
			// Perform file
			self.extent = off;
		}
		else {
			println!("{} {}", size, off);

			eprintln!("Detected incomplete data at end of file");

			// Truncating to the end of the last file (we will just overwrite the existing data when we start appending more data)
			self.extent = last_off;
		}

		// Flush in case we added orphans to the index
		self.index_file.flush()?;

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

		self.file.seek(io::SeekFrom::Start(entry.offset(self.superblock.block_size)))?;

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
		

		// Seek to the end of the file (and get that offset)
		// TODO: Instead we should be tracking the end as the offset after the last known good needle (as we don't want to compound corruptions)
		let off = self.extent;
		self.file.seek(io::SeekFrom::Start(off))?;

		if off % self.superblock.block_size != 0 {
			return Err("File not block aligned".into());
		}

		let block_offset = (off / self.superblock.block_size) as BlockOffset;


		let header: Vec<u8> = NeedleHeader::serialize(cookie.data(), &keys, &meta)?;


		let mut next_extent = off + (header.len() + (meta.size as usize) + NEEDLE_FOOTER_SIZE) as u64;
		let rem = block_size_remainder(self.superblock.block_size, next_extent);
		next_extent = next_extent + rem;

		// TODO: Should we reject needles that go over the allocation size right here? (currently it is only enforced in the routes layer before the needle is written)

		// Take control of the filesystem allocation process so long as we are not hitting our overall filesystem limit
		// TODO: Another optimization would be preallocate a large amount of space all at once when we are doing compactions as they have a pretty well known size
		if next_extent > self.preallocated {

			// Round up to the next preallocation block size
			let mut next_preallocated = next_extent
				+ block_size_remainder(self.config.store.preallocate_size, next_extent);

			// Current estimate of total size needed to store the index when full
			let index_space = self.predicted_index_size();

			// Using this measurement, the remainder of the space should be left for the data file
			let space_for_volume = self.superblock.allocated_space - index_space;

			// Should never allocate more than the total allocated space for the volume
			if space_for_volume < next_preallocated {
				next_preallocated = space_for_volume
			}

			// Must at least allocate enough space to store the current needle
			if next_extent > next_preallocated {
				// TODO: Near the end of the volume, this condition may get hit a lot and result in many small inefficient allocation
				next_preallocated = next_extent;
			}

			allocate_soft(&self.file, next_preallocated)?;
			self.preallocated = next_preallocated;
		}


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

		// Create the footer (+ pad to the next block)
		let mut footer_buf = Vec::new();
		footer_buf.resize(NEEDLE_FOOTER_SIZE + rem as usize, 0);
		NeedleFooter::write(&mut Cursor::new(&mut footer_buf), sum)?;
		
		self.file.write_all(&footer_buf)?;

		// Mark the new end of the file
		self.extent = next_extent;

		self.add_to_index(keys.clone(), NeedleIndexEntry {
			meta: meta.clone(),
			block_offset
		}, false)?;

		Ok(())
	}

	pub fn predicted_index_size(&self) -> u64 {
		let mut index_extent = self.index_file.used_space();
		
		// When the index file is big enough that actual entries overweight the size of the header metadata, we will try forward predicting the size of the index file
		if self.num_needles() > 128 {

			// Based on the current index-space to data-space ratio, calculate how large we expect the index file to be near max capacity
			
			let index_percent = (index_extent as f64) / ((index_extent + self.extent) as f64);
			if index_percent > 0.05 {
				eprintln!("Extremely dense index file");
			}
			else {
				let index_predicted_size = (index_percent * (self.superblock.allocated_space as f64)) as u64;

				// Sanity check the measurement (we should never predict less space than is currently being used)
				if index_predicted_size > index_extent {
					index_extent = index_predicted_size;
				}
			}
		}

		index_extent
	}

	pub fn delete_needle(&mut self, keys: &NeedleKeys) -> Result<()> {

		let entry = match self.index.get(keys) {
			Some(e) => e,
			None => return Err("Needle does not exist".into()),
		};

		if entry.meta.deleted() {
			return Err("Needle already deleted".into());
		}


		// TODO: Whenever we see a deleted needle, we can add it to our pending compaction count

		//entry.offset.

		// read the header in the file

		// double check it's flag isn't already set

		// write back to the file in place

		Ok(())
	}

	/// Flushes the volume such that any recent append_needle operations persist to disk
	pub fn flush(&mut self) -> Result<()> {
		// TODO: If we were really crazy about performance, we could count how many needles not yet flushed and perform a flush only if everything isn't already flushed
		self.file.sync_data()?;
		Ok(())
	}

	pub fn close(mut self) -> Result<()> {
		// NOTE: In general, this should always have been already handled by someone else
		self.flush()?;

		self.index_file.flush()
	}

	fn pad_to_block_size(&mut self) -> Result<u64> {
		let pos = self.file.seek(io::SeekFrom::Current(0))?;
		let pad = block_size_remainder(self.superblock.block_size, pos);
		if pad != 0 {
			let mut padding = Vec::new();
			padding.resize(pad as usize, 0);
			self.file.write_all(&padding)?;
		}

		Ok(pos + pad)
	}

	fn offset_after_super_block(&self) -> u64 {
		let mut off = SUPERBLOCK_SIZE as u64;
		off += block_size_remainder(self.superblock.block_size, off);
		off
	}

}


#[cfg(test)]
mod tests {

	use super::*;
	use super::super::stream::SingleStream;
	use std::fs;
	use std::sync::Arc;

	#[test]
	fn physical_volume_append() -> Result<()> {

		// TODO: Also clear the index?
		let p = Path::new("out/teststore");
		if p.exists() {
			fs::remove_file(&p)?;
		}

		let config = Arc::new(Config::default());

		// Create new with single needle
		{
			let mut vol = PhysicalVolume::create(config.clone(), &p, 123, 456, 7)?;

			let keys = NeedleKeys { key: 22, alt_key: 3 };

			let r = vol.read_needle(&keys)?;
			assert!(r.is_none());
			assert_eq!(vol.num_needles(), 0);

			let data = vec![1,2,3,4,3,2,1];
			let meta = NeedleMeta {
				flags: 0,
				size: data.len() as NeedleSize
			};
			let cookie = CookieBuf::random();
			let mut strm = SingleStream::from(&data);

			vol.append_needle(keys.clone(), cookie.clone(), meta, &mut strm)?;


			let r2 = vol.read_needle(&keys)?;
			assert!(r2.is_some());

			let n = r2.unwrap();
			assert_eq!(n.block_offset, 1);
			assert_eq!(n.needle.data(), &data[..]);

			assert_eq!(vol.num_needles(), 1);
		}

		// Reopen
		{
			let mut vol = PhysicalVolume::open(config.clone(), &p)?;
			assert_eq!(vol.superblock.cluster_id, 123);
			assert_eq!(vol.superblock.machine_id, 456);
			assert_eq!(vol.superblock.volume_id, 7);

			assert_eq!(vol.num_needles(), 1);
		}

		Ok(())
	}

}

