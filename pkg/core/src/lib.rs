extern crate fs2;
extern crate libc;

pub mod allocate_soft;


use std::fs::{File, OpenOptions};
use fs2::FileExt;
use std::path::{Path, PathBuf};


pub trait FlipSign<T> {
	/// Transmutes an signed/unsigned integer into it's opposite unsigned/signed integer while maintaining bitwise equivalence even though the integer value may change
	/// 
	/// We use this rather than directly relying on 'as' inline to specify times when we intentionally don't care about the value over/underflowing upon reinterpretation of the bits in a different sign
	fn flip(self) -> T;
}

impl FlipSign<u16> for i16 { fn flip(self) -> u16 { self as u16 } }
impl FlipSign<i16> for u16 { fn flip(self) -> i16 { self as i16 } }
impl FlipSign<u32> for i32 { fn flip(self) -> u32 { self as u32 } }
impl FlipSign<i32> for u32 { fn flip(self) -> i32 { self as i32 } }
impl FlipSign<u64> for i64 { fn flip(self) -> u64 { self as u64 } }
impl FlipSign<i64> for u64 { fn flip(self) -> i64 { self as i64 } }




/// Given that the current position in the file is at the end of a middle, this will determine how much 
pub fn block_size_remainder(block_size: u64, end_offset: u64) -> u64 {
	let rem = end_offset % block_size;
	if rem == 0 {
		return 0;
	}

	(block_size - rem)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn block_size_remainder_test() {
		let bsize = 64;
		assert_eq!(block_size_remainder(bsize, 0), 0);
		assert_eq!(block_size_remainder(bsize, 3*bsize), 0);
		assert_eq!(block_size_remainder(bsize, bsize - 4), 4);
		assert_eq!(block_size_remainder(bsize, 6*bsize + 5), bsize - 5);
	}

}


// TODO: Better error passthrough?

/// Allows for holding an exclusive lock on a directory
/// 
/// TODO: Eventually we should require that most file structs get opened using a DirLock or a path derived from a single DirLock to gurantee that only one struct/process has access to it
pub struct DirLock {
	/// File handle for the lock file that we create to hold the lock
	/// NOTE: Even if we don't use this, it must be held allocated to maintain the lock
	_file: File,

	/// Extra reference to the directory path that we represent
	path: PathBuf
}

impl DirLock {

	/// Locks an new directory
	/// 
	/// TODO: Support locking based on an application name which we could save in the lock file
	pub fn open(path: &Path) -> Result<DirLock, &'static str> {
		if !path.exists() {
			return Err("Folder does not exist");
		}

		let lockfile_path = path.join(String::from("lock"));

		// Before we create a lock file, verify that the directory is empty (partially ensuring that all previous owners of this directory also respected the locking rules)
		if !lockfile_path.exists() {
			let nfiles = path.read_dir()
				.map_err(|_| "Failed to read the given directory")?
				.collect::<Vec<_>>().len();

			if nfiles > 0 {
				return Err("Folder is not empty".into());
			}
		}

		let mut opts = OpenOptions::new();
		opts.write(true).create(true).read(true);

		let lockfile = opts.open(lockfile_path).map_err(|_| "Failed to open the lockfile")?;

		// Acquire the exclusive lock
		if let Err(_) = lockfile.try_lock_exclusive() {
			return Err("Failed to lock the lockfile");
		}

		Ok(DirLock {
			_file: lockfile,
			path: path.to_owned()
		})
	}

	pub fn path(&self) -> &Path {
		&self.path
	}


}
