use super::super::common::*;
use super::super::errors::*;
use super::needle::*;
use super::superblock::*;
use std::path::Path;
use std::fs::{File, OpenOptions};
use std::mem::size_of;
use byteorder::{LittleEndian, WriteBytesExt, ReadBytesExt};
use std::io::{Read, Write, Seek, Cursor, SeekFrom};
use super::block_size_remainder;

const SUPERBLOCK_MAGIC: &str = "HAYI";

/// An index for all of the entries in a physical volume
pub struct PhysicalVolumeIndex {

	pub superblock: PhysicalVolumeSuperblock,

	file: File,

	/// Offset in the file to the end of the last known good entry
	extent: u64,

	// Number of entries that we have written but are not flushed to disk
	pending: usize
}


const PAIR_SIZE: usize =
	size_of::<NeedleKey>() +
	size_of::<NeedleAltKey>() +
	1 + // Flags
	size_of::<BlockOffset>() +
	size_of::<NeedleSize>(); // Size of the needle


pub struct NeedleIndexPair {
	pub keys: NeedleKeys,
	pub value: NeedleIndexEntry
}

impl NeedleIndexPair {
	pub fn read(reader: &mut Read) -> Result<NeedleIndexPair> {

		let key = reader.read_u64::<LittleEndian>()?;
		let alt_key = reader.read_u32::<LittleEndian>()?;
		let flags = reader.read_u8()?;
		let block_offset = reader.read_u32::<LittleEndian>()?;
		let size = reader.read_u64::<LittleEndian>()?;

		Ok(NeedleIndexPair {
			keys: NeedleKeys { key, alt_key },
			value: NeedleIndexEntry {
				meta: NeedleMeta {
					flags,
					size
				},
				block_offset
			}
		})
	}

	pub fn write(keys: &NeedleKeys, value: &NeedleIndexEntry, writer: &mut Write) -> Result<()> {
		writer.write_u64::<LittleEndian>(keys.key)?;
		writer.write_u32::<LittleEndian>(keys.alt_key)?;
		writer.write_u8(value.meta.flags)?;
		writer.write_u32::<LittleEndian>(value.block_offset)?;
		writer.write_u64::<LittleEndian>(value.meta.size)?;
		Ok(())
	}
}

/*
	TODO: WE probably need to checksum this thing in some way
*/

impl PhysicalVolumeIndex {

	/// Create a brand new empty index
	pub fn create(
		path: &Path, cluster_id: ClusterId, machine_id: MachineId, volume_id: VolumeId
	) -> Result<PhysicalVolumeIndex> {
		
		// NOTE: The index is redundant to the main file, so it's easiest to just truncate any existing volumes in the case of newly created volumes
		let mut opts = OpenOptions::new();
		opts.write(true).create(true).truncate(true).read(true);

		let mut file = opts.open(path)?;

		let superblock = PhysicalVolumeSuperblock {
			magic: SUPERBLOCK_MAGIC.as_bytes().into(),
			machine_id,
			volume_id,
			cluster_id
		};

		superblock.write(&mut file)?;
		file.flush()?;

		let idx = PhysicalVolumeIndex {
			superblock,
			file,
			extent: (SUPERBLOCK_SIZE as u64),
			pending: 0
		};

		Ok(idx)
	}

	/// Open an existing physical volume index
	/// NOTE: read_all must be run before appending entries to this file
	pub fn open(path: &Path) -> Result<PhysicalVolumeIndex> {

		let mut opts = OpenOptions::new();
		opts.read(true).write(true);

		let mut file = opts.open(path)?;

		let mut header = [0u8; SUPERBLOCK_SIZE];
		file.read_exact(&mut header)?;

		let superblock = PhysicalVolumeSuperblock::read(&mut Cursor::new(&header))?;

		if &superblock.magic[..] != SUPERBLOCK_MAGIC.as_bytes() {
			return Err("Superblock magic is incorrect".into());
		}

		let mut idx = PhysicalVolumeIndex {
			superblock,
			file,
			extent: (SUPERBLOCK_SIZE as u64),
			pending: 0
		};

		Ok(idx)
	}

	pub fn read_all(&mut self, volume_max_extent: u64) -> Result<Vec<NeedleIndexPair>> {

		let mut out: Vec<NeedleIndexPair> = vec![];

		self.file.seek(SeekFrom::Start(SUPERBLOCK_SIZE as u64))?;

		let mut len = self.file.metadata()?.len() - (SUPERBLOCK_SIZE as u64);

		let rem = len % (PAIR_SIZE as u64);
		if rem != 0 {
			eprintln!("Detected partially flushed index file");
		}


		let n = len / (PAIR_SIZE as u64);

		// XXX: Not good to leave on heap
		let mut buf = Vec::new();
		buf.resize(len as usize, 0);
		self.file.read_exact(&mut buf)?;

		let mut c = Cursor::new(buf);

		let mut off = SUPERBLOCK_SIZE;

		for _ in 0..n {
			let pair = NeedleIndexPair::read(&mut c)?;

			// Sanity check: offsets should be non-overlapping and contiguous
			if let Some(last_pair) = out.last() {
				// NOTE: Must technically also pad it up to fit everything

				let off = last_pair.value.end_offset();
				if off != pair.value.offset() {
					return Err("Corrupt non-contiguous index file entries".into());
				}
			}
			else {
				if pair.value.block_offset != 1 {
					return Err("First entry in index file does not start right after the superblock".into());
				}
			}

			// Verify that no entry in the index file would go beyond the size of the main volume file
			// This will generally happen if the index file is flushed before the main volume file
			// This doesn't really matter but is just a biproduct of us not qeueing index entries in batch upload scenarios as it doesn't really matter all that much
			let end_off = pair.value.end_offset();
			if end_off > volume_max_extent {
				eprintln!("Index file contains entries beyond the end of the main volume");
				self.file.set_len(off as u64)?;
				break;
			}

			off = off + PAIR_SIZE;
			out.push(pair);
		}

		self.extent = off as u64;

		Ok(out)
	}

	pub fn append(&mut self, keys: &NeedleKeys, value: &NeedleIndexEntry) -> Result<()> {
		
		self.file.seek(SeekFrom::Start(self.extent))?;
		
		let mut buf = Vec::new();
		buf.reserve(PAIR_SIZE);
		
		NeedleIndexPair::write(keys, value, &mut Cursor::new(&mut buf))?;

		// TODO: Probably best to not write anything right away but instead wait until the changes we have pending will make the file some size just under the FS block size and the nwrite and flush everything
		self.file.write_all(&buf)?;

		self.extent = self.extent + (PAIR_SIZE as u64);
		self.pending = self.pending + 1;

		if self.pending > 10 {
			self.flush()?;
		}

		Ok(())
	}

	/// Forces all pending writes to commit to disk
	pub fn flush(&mut self) -> Result<()> {
		if self.pending == 0 {
			return Ok(());
		}

		self.file.flush()?;
		self.pending = 0;
		Ok(())
	}

	/// Gets the last entry in this index
	/// NOTE: Should only be relied on when the index is synced with the volume it came from and is flushed to disk
	pub fn last_entry(&mut self) -> Result<NeedleIndexPair> {
		self.file.seek(SeekFrom::Start(self.extent - (PAIR_SIZE as u64)))?;
		Ok(NeedleIndexPair::read(&mut self.file)?)
	}


}

