use core::ffi::CStr;

use alloc::{ffi::CString, string::String, vec::Vec};

use common::errors::*;
use executor::RemapErrno;

use crate::{FileError, LocalFile, LocalPath};

pub type FileType = sys::FileType;

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

    let dir = LocalFile::open(path)?;

    let mut buffer = [0u8; 8192];

    loop {
        let mut rest =
            unsafe { sys::getdents64(dir.as_raw_fd(), &mut buffer) }.remap_errno::<FileError>()?;
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
