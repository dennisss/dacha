/*
	Derived from original code in https://github.com/danburkert/fs2-rs/blob/master/src/unix.rs and https://github.com/danburkert/fs2-rs/blob/master/src/windows.rs

	- This provides a version of fs2's allocate() that does not modify the size of the file's contents (but only the allocated space)

	- NOTE: Currently this just contains the linux and mac implementations

	- NOTE: This is also optimized for the append-only case, such that if the given size is less than the current size of the file, then we won't bother trying to make is smaller
*/

use std::fs::File;
use std::io::{Error, Result};
use std::os::unix::io::{AsRawFd};


#[cfg(any(target_os = "linux", target_os = "android"))]
pub fn allocate_soft(file: &file, len: u64) -> Result<()> {
	let ret = unsafe {
		libc::fallocate(file.as_raw_fd(), libc::FALLOC_FL_KEEP_SIZE, 0, len as libc::off_t)
	};

	if ret == 0 { Ok(()) } else { Err(Error::last_os_error()) }
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub fn allocate_soft(file: &File, len: u64) -> Result<()> {

	let mut fstore = libc::fstore_t {
		fst_flags: libc::F_ALLOCATECONTIG,
		fst_posmode: libc::F_PEOFPOSMODE,
		fst_offset: 0,
		fst_length: len as libc::off_t,
		fst_bytesalloc: 0,
	};

	let ret = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_PREALLOCATE, &fstore) };
	if ret == -1 {
		// Unable to allocate contiguous disk space; attempt to allocate non-contiguously.
		fstore.fst_flags = libc::F_ALLOCATEALL;
		let ret = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_PREALLOCATE, &fstore) };
		if ret == -1 {
			return Err(Error::last_os_error());
		}
	}

	Ok(())
}
