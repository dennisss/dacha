use core::ffi::CStr;

use alloc::{ffi::CString, string::String, vec::Vec};

use common::errors::*;
use executor::RemapErrno;
use sys::OpenFileDescriptor;

use crate::{FileError, LocalFile, LocalPath};

pub type FileType = sys::FileType;

/*
We'd ideally like to be able to propagate a file system implementation that forces strict syncronization of all data in some directory.

*/

/*
Need to implement an appendable file wrapper which buffers the last page of data for O_DIRECT un-aligned writes

- In some cases, it might be better to pad the file than to append though.
- We also don't need to re-write it unless we are flushing.

Note that we don't want it to implement readable (otherwise we're getting into the same issues as the linux page cache)

For synced io, we must validate that a file exists using O_DIRECT

*/

/*
pub struct LocalDirectory {
    file: OpenFileDescriptor,
}

impl LocalDirectory {
    pub fn open<P: AsRef<LocalPath>>(path: P) -> Result<Self> {
        let cpath = CString::new(path.as_ref().as_str())?;

        // TODO: Make file errors. Also do it in LocalFile::open
        let fd = unsafe { sys::open(cpath.as_ptr(), sys::O_RDONLY | sys::O_DIRECTORY, 0) }?;

        let file = OpenFileDescriptor::new(fd);

        Ok(Self { file })
    }

    //
}
*/

#[derive(Debug, Clone)]
pub struct LocalDirEntry {
    inode: u64,
    name: String,
    typ: FileType,
}

impl LocalDirEntry {
    pub fn typ(&self) -> FileType {
        self.typ
    }

    /// TODO: Rename to 'file_name'
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// This will return an error if the path is not a directory.
///
/// TODO: Test this with an empty
pub fn read_dir<P: AsRef<LocalPath>>(path: P) -> Result<Vec<LocalDirEntry>> {
    // TODO: Check if the file is actually a directory?

    let mut out = vec![];

    // TODO: If we are checking critical files, it makes sense it us O_DIRECT here?
    let dir = LocalFile::open(path)?;

    let mut buffer = [0u8; 8192];

    loop {
        let mut rest = unsafe { sys::getdents64(dir.as_raw_fd(), &mut buffer) }
            .remap_errno::<FileError, _>(|| String::new())?;
        if rest.is_empty() {
            break;
        }

        // let mut saw_last = false;
        while !rest.is_empty() {
            let (dirent, r) = sys::DirEntry::parse(rest);
            rest = r;

            let mut null_term_pos = None;
            for i in 0..dirent.name.len() {
                if dirent.name[i] == 0 {
                    null_term_pos = Some(i);
                    break;
                }
            }

            let null_term_pos =
                null_term_pos.ok_or_else(|| err_msg("Name missing null termiantor"))?;

            let name = String::from_utf8(dirent.name[..null_term_pos].to_vec())?;

            if name == "." || name == ".." {
                continue;
            }

            out.push(LocalDirEntry {
                inode: dirent.inode,
                name,
                typ: dirent.typ,
            });
        }
    }

    Ok(out)
}

/*
Use 'readlink'

*/
