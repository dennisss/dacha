extern crate fs2;

use std::fs::{File, OpenOptions};
use fs2::FileExt;
use std::path::{Path, PathBuf};


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
