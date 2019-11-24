use super::errors::*;
use std::io::{SeekFrom, Write, Read, Seek};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use bytes::Bytes;
use byteorder::{ReadBytesExt, WriteBytesExt, LittleEndian};
use crc32c::crc32c_append;
use std::os::unix::io::{AsRawFd};
use std::io;

/// Amount of padding that we add to the file for the length and checksume bytes
const PADDING: u64 = 8;

const DISK_SECTOR_SIZE: u64 = 512;

/*
	Cases to test:
	- Upon a failed creation, we should not report the file as created
		- Successive calls to create() should be able to delete any partially created state

*/

/*
	The read index in the case of the memory store
	-> Important notice 

*/

/*
	NOTE: etcd/raft assumes that the entire snapshot fits in memory
	-> Not particularly good
	-> Fine as long as limit range sizes for 
*/

// Simple case is to just generate a callback

// https://docs.rs/libc/0.2.48/libc/fn.unlinkat.html
// TODO: Also linux's rename will atomically replace any overriden file so we could use this fact to remove one more syscall from the process


/// Wraps a binary blob that can be atomically read/written from the disk 
/// Additionally this will add some checksumming to the file to verify the integrity of the data and accept/reject partial reads
/// 
/// NOTE: If any operation fails, then this struct should be considered poisoned and unuseable
/// 
/// NOTE: This struct does not deal with maintaining an internal buffer of the current value, so that is someone elses problem as this is meant to be super light weight
/// 
/// NOTE: This assumes that this object is being given exclusive access to the given path (meaning that the directory is locked)
pub struct BlobFile {

	// TODO: Would also be good to know the size of it 

	/// Cached open file handle to the directory containing the file
	dir: File,

	/// The path to the main data file this uses
	path: PathBuf,

	/// Path to temporary data file used to store the old data value until the new value is fully written
	path_tmp: PathBuf,

	/// Path to a temporary file used only during initial creation of the file
	/// It will only exist if the file has never been successfully created before
	path_new: PathBuf
}

pub struct BlobFileBuilder {
	inner: BlobFile
}


// TODO: For unlinks, unlinkat would probably be most efficient using a relative path
// XXX: Additionally openat for 

// Writing will always create a new file right?

// TODO: open must distinguish between failing to read existing data and failing because it doesn't exist 

impl BlobFile {

	// TODO: If I wanted to be super Rusty, I could represent whether or not it exists (i.e. whether create() or open() should be called) by returning an enum here instead of relying on the user checking the value of exists() at runtime
	pub fn builder(path: &Path) -> Result<BlobFileBuilder> {
		let path = path.to_owned();
		let path_tmp = PathBuf::from(&(path.to_str().unwrap().to_owned() + ".tmp"));
		let path_new = PathBuf::from(&(path.to_str().unwrap().to_owned() + ".new"));

		let dir = {
			let path_dir = match path.parent() {
				Some(p) => p,
				None => return Err("Path is not in a directory".into())
			};

			if !path_dir.exists() {
				return Err("Directory does not exist".into());
			}

			File::open(&path_dir)?
		};
			
		Ok(BlobFileBuilder {
			inner: BlobFile {
				dir, path, path_tmp, path_new
			}
		})
	}
	
	/// Overwrites the file with a new value (atomically of course)
	pub fn store(&self, data: &[u8]) -> Result<()> {
		
		let new_filesize = (data.len() as u64) + PADDING;

		// Performant case of usually only requiring one sector write to replace the file (aside from possibly needing to change the length of it)
		// TODO: We could just make sure that these files are always at least 512bytes in length in order to avoid having to do all of the truncation and length changes
		// TODO: Possibly speed up by caching the size of the old file?
		if new_filesize < DISK_SECTOR_SIZE {
			let old_filesize = self.path.metadata()?.len();

			let mut file = OpenOptions::new().write(true).open(&self.path)?;

			if new_filesize > old_filesize {
				file.set_len(new_filesize)?;
				file.sync_data()?;
			}

			Self::write_simple(&mut file, data)?;

			if new_filesize < old_filesize {
				file.set_len(new_filesize)?;
			}

			file.sync_data()?;

			return Ok(());
		}

		// Rename old value
		std::fs::rename(&self.path, &self.path_tmp)?;
		self.dir.sync_data()?;

		// Write new value
		let mut file = OpenOptions::new().write(true).create_new(true).open(&self.path)?;
		Self::write_simple(&mut file, data)?;
		file.sync_data()?;
		self.dir.sync_data()?;

		// Remove old value
		/*
		// Basically must cache the actual file name
		{
			if cfg!(any(target_os = "linux")) {
				let ret = unsafe {
					libc::fallocate(file.as_raw_fd(), libc::FALLOC_FL_KEEP_SIZE, 0, len as libc::off_t)
				};

				if ret == 0 { Ok(()) } else { Err(Error::last_os_error()) }
			}
			else {

			}
		}
		*/


		std::fs::remove_file(&self.path_tmp)?;

		// NOTE: A dir sync should not by needed here

		Ok(())
	}


	fn write_simple(file: &mut File, data: &[u8]) -> Result<u64> {
		let sum = crc32c_append(0, data);

		file.seek(SeekFrom::Start(0))?;
		file.write_u32::<LittleEndian>(data.len() as u32)?;
		file.write_all(data)?;
		file.write_u32::<LittleEndian>(sum)?;

		let pos = file.seek(SeekFrom::Current(0))?;
		assert_eq!(pos, (data.len() as u64) + PADDING);

		Ok(pos)
	}

}

impl BlobFileBuilder {

	pub fn exists(&self) -> bool {
		self.inner.path.exists() || self.inner.path_tmp.exists()
	}

	/// If any existing data exists, this will delete it
	pub fn purge(&self) -> Result<()> {
		if self.inner.path.exists() {
			std::fs::remove_file(&self.inner.path)?;
		}
		
		if self.inner.path_tmp.exists() {
			std::fs::remove_file(&self.inner.path_tmp)?;
		}

		if self.inner.path_new.exists() {
			std::fs::remove_file(&self.inner.path_new)?;
		}

		Ok(())
	}

	/// Opens the file assuming that it exists
	/// Errors out if we could be not read the data because it is corrupt or non-existent
	pub fn open(self) -> Result<(BlobFile, Bytes)> {
		if !self.exists() {
			return Err("File does not exist".into());
		}

		let inst = self.inner;

		if inst.path.exists() {			
			let res = Self::try_open(&inst.path)?;
			if let Some(data) = res {
				if inst.path_tmp.exists() {
					std::fs::remove_file(&inst.path_tmp)?;
					inst.dir.sync_all()?;
				}

				return Ok((inst, data));
			}
		}

		if inst.path_tmp.exists() {
			let res = Self::try_open(&inst.path_tmp)?;
			if let Some(data) = res {
				if inst.path.exists() {
					std::fs::remove_file(&inst.path)?;
				}

				std::fs::rename(&inst.path_tmp, &inst.path)?;
				inst.dir.sync_all()?;

				return Ok((inst, data));
			}
		}

		Err("No valid data could be read (corrupt data)".into())
	}

	/// Tries to open the given path
	/// Returns None if the file doesn't contain valid data in it
	fn try_open(path: &Path) -> Result<Option<Bytes>> {

		let mut file = File::open(path)?;

		let mut buf = vec![];
		file.read_to_end(&mut buf)?;


		let length = {
			if buf.len() < 4 { return Ok(None); }
			(&buf[0..4]).read_u32::<LittleEndian>()? as usize
		};

		let data_start = 4;
		let data_end = data_start + length;
		let checksum_end = data_end + 4;

		assert_eq!(checksum_end, length + (PADDING as usize));

		if buf.len() < checksum_end { return Ok(None); }

		let sum = crc32c_append(0, &buf[data_start..data_end]);
		let expected_sum = (&buf[data_end..checksum_end]).read_u32::<LittleEndian>()?;

		if sum != expected_sum {
			return Ok(None);
		}

		// File is larger than its valid contents (we will just truncate it)
		if buf.len() > checksum_end {
			file.set_len(data_end as u64)?;
		}

		let bytes = Bytes::from(buf);

		Ok(Some(bytes.slice(data_start, data_end)))
	}

	/// Creates a new file with the given initial value
	/// Errors out if any data already exists or if the write fails
	pub fn create(self, initial_value: &[u8]) -> Result<BlobFile> {
		
		if self.exists() {
			return Err("Existing data already exists".into());
		}
		
		let inst = self.inner;

		// This may occur if we previously tried creating a data file but we were never able to suceed
		if inst.path_new.exists() {
			std::fs::remove_file(&inst.path_new)?;
		}

		let mut opts = OpenOptions::new();
		opts.write(true).create_new(true);

		let mut file = opts.open(&inst.path_new)?;

		BlobFile::write_simple(&mut file, initial_value)?;
		file.sync_all()?;

		std::fs::rename(&inst.path_new, &inst.path)?;

		inst.dir.sync_all()?;

		Ok(inst)
	}
	
}

