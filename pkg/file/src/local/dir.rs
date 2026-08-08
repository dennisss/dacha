use core::ffi::CStr;

use alloc::{ffi::CString, string::String, vec::Vec};

use common::errors::*;
use executor::error::*;

use crate::{FileError, LocalFile, LocalPath};

#[cfg(target_os = "linux")]
pub type FileType = sys::FileType;

#[cfg(not(target_os = "linux"))]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileType {
    RegularFile,
    Directory,
    SymbolicLink,
    Unknown
}

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
    #[cfg(target_os = "linux")]
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
#[cfg(target_os = "linux")]
pub fn read_dir<P: AsRef<LocalPath>>(path: P) -> Result<Vec<LocalDirEntry>> {
    // TODO: Check if the file is actually a directory?

    let path = path.as_ref();

    let mut out = vec![];

    // TODO: If we are checking critical files, it makes sense it us O_DIRECT here?
    let dir = LocalFile::open(path)?;

    let mut buffer = [0u8; 8192];

    loop {
        let mut rest = unsafe { sys::getdents64(dir.as_raw_fd(), &mut buffer) }
            .remap_errno::<FileError, _>(|| format!("getdents64(\"{}\")", path.as_str()))?;
        if rest.is_empty() {
            break;
        }

        // let mut saw_last = false;
        while !rest.is_empty() {
            let (dirent, r) = sys::DirEntry::parse(rest);
            rest = r;

            let name = base_util::null_terminated::read_null_terminated_string(&dirent.name[..])?;

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

#[cfg(target_os = "windows")]
pub fn read_dir<P: AsRef<LocalPath>>(path: P) -> Result<Vec<LocalDirEntry>> {
    let mut out = vec![];
    
    let iter = std::fs::read_dir(path)
        .remap_std_error::<FileError, _>(|| "".into())?;

    for entry in iter {
        let entry = entry.remap_std_error::<FileError, _>(|| "".into())?;

        let file_type = entry.file_type().remap_std_error::<FileError, _>(|| "".into())?;

        out.push(LocalDirEntry {
            name: entry.file_name().to_str().unwrap().into(),
            typ: {
                if file_type.is_file() {
                    FileType::RegularFile
                } else if file_type.is_dir() {
                    FileType::Directory
                } else if file_type.is_symlink() {
                    FileType::SymbolicLink
                } else {
                    FileType::Unknown
                }
            }
        });
    }

    Ok(out)
}

/*
Use 'readlink'

*/
