use core::ffi::CStr;

use alloc::{ffi::CString, string::String, vec::Vec};

use common::io::Readable;
use common::{errors::*, io::Writeable};
use executor::RemapErrno;
use sys::Errno;

use crate::{
    FileError, FileType, LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf, Metadata,
    Permissions,
};

pub async fn read<P: AsRef<LocalPath>>(path: P) -> Result<Vec<u8>> {
    let mut out = vec![];
    let mut file = LocalFile::open(path)?;
    file.read_to_end(&mut out).await?;
    Ok(out)
}

pub async fn read_to_string<P: AsRef<LocalPath>>(path: P) -> Result<String> {
    let mut out = vec![];
    let mut file = LocalFile::open(path)?;
    file.read_to_end(&mut out).await?;

    Ok(String::from_utf8(out)?)
}

pub fn current_dir() -> Result<LocalPathBuf> {
    let mut buffer = vec![0u8; 1024];
    let n = unsafe { sys::getcwd(&mut buffer).remap_errno::<FileError>()? };

    if n > buffer.len() || n < 1 || buffer[n - 1] != 0 {
        return Err(err_msg("Expected null terminator in cwd"))?;
    }

    buffer.truncate(n - 1);

    Ok(LocalPathBuf::from(String::from_utf8(buffer)?))
}

/// NOTE: THis results the link as-is which may be a relative path to the
/// containing directory.
pub fn readlink<P: AsRef<LocalPath>>(path: P) -> Result<LocalPathBuf> {
    let mut buffer = [0u8; 4096];

    let path = CString::new(path.as_ref().as_str())?;
    let n = unsafe { sys::readlink(path.as_ptr() as *const u8, &mut buffer) }
        .remap_errno::<FileError>()?;

    let s = String::from_utf8(buffer[..n].to_vec())?;

    Ok(LocalPathBuf::from(s))
}

/// Based on the example: https://doc.rust-lang.org/std/fs/fn.read_dir.html#examples
pub fn recursively_list_dir(dir: &LocalPath, callback: &mut dyn FnMut(&LocalPath)) -> Result<()> {
    for entry in crate::read_dir(dir)? {
        // TODO: Consider following symlinks.
        // Need to resolve a symlink to it's full path.

        let path = dir.join(entry.name());

        if entry.typ() == FileType::Directory {
            recursively_list_dir(&path, callback)?;
        } else {
            callback(&path);
        }
    }

    Ok(())
}

pub async fn metadata(path: &LocalPath) -> Result<Metadata> {
    let path = CString::new(path.as_str())?;
    let mut stat = sys::bindings::stat::default();
    unsafe { sys::stat(path.as_ptr() as *const u8, &mut stat) }.remap_errno::<FileError>()?;
    Ok(Metadata { inner: stat })
}

pub async fn symlink_metadata(path: &LocalPath) -> Result<Metadata> {
    let path = CString::new(path.as_str())?;
    let mut stat = sys::bindings::stat::default();
    unsafe { sys::lstat(path.as_ptr() as *const u8, &mut stat) }.remap_errno::<FileError>()?;
    Ok(Metadata { inner: stat })
}

/// TODO: This only applies to some operations. It might make more sense to have
/// operation specific error variants.
pub fn because_file_doesnt_exist(error: &Error) -> bool {
    if let Some(err) = error.downcast_ref::<FileError>() {
        match err {
            FileError::NotFound | FileError::NotADirectory | FileError::InvalidPath => true,
            _ => false,
        }
    } else {
        false
    }
}

/// TODO: Test that this can distinguish between a normal not_found and a
/// permission error.
pub async fn exists<P: AsRef<LocalPath>>(path: P) -> Result<bool> {
    // TODO: Use symlink_metadata?
    match metadata(path.as_ref()).await {
        Ok(_) => Ok(true),
        Err(e) => {
            if because_file_doesnt_exist(&e) {
                return Ok(false);
            }

            Err(e.into())
        }
    }
}

pub async fn create_dir(path: &LocalPath) -> Result<()> {
    let path = CString::new(path.as_str())?;
    unsafe { sys::mkdir(path.as_ptr() as *const u8, 0o777).remap_errno::<FileError>()? }
    Ok(())
}

pub async fn create_dir_all(path: &LocalPath) -> Result<()> {
    let mut stack = vec![];

    // We need to normalize this to ensure that every parent path is actually the
    // parent directory of the current one.
    let normalized_path = path.normalized();

    let mut cur = Some(path);
    while let Some(p) = cur {
        if exists(p).await? {
            break;
        }

        stack.push(p);
        cur = p.parent();
    }

    while let Some(p) = stack.pop() {
        create_dir(p).await?;
    }

    Ok(())
}

pub async fn set_permissions(path: &LocalPath, perms: Permissions) -> Result<()> {
    let path = CString::new(path.as_str())?;
    unsafe { sys::chmod(path.as_ptr() as *const u8, perms.mode).remap_errno::<FileError>()? }
    Ok(())
}

pub async fn write<P: AsRef<LocalPath>, V: AsRef<[u8]>>(path: P, value: V) -> Result<()> {
    let mut file = LocalFile::open_with_options(
        path,
        &LocalFileOpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true),
    )?;
    file.write_all(value.as_ref()).await?;

    Ok(())
}

pub async fn remove_dir<P: AsRef<LocalPath>>(path: P) -> Result<()> {
    let path = path.as_ref();
    let path = CString::new(path.as_str())?;

    unsafe { sys::rmdir(path.as_ptr() as *const u8).remap_errno::<FileError>()? };
    Ok(())
}

pub async fn remove_file<P: AsRef<LocalPath>>(path: P) -> Result<()> {
    let path = path.as_ref();
    let path = CString::new(path.as_str())?;

    unsafe { sys::unlink(path.as_ptr() as *const u8).remap_errno::<FileError>()? };
    Ok(())
}

pub async fn remove_dir_all<P: AsRef<LocalPath>>(path: P) -> Result<()> {
    // NOTE: We should use symlink_metadata to avoid deleting things across
    // symlinks.


    todo!()
}

/// Moves the file currently located at 'from' to 'to'
pub async fn rename<P: AsRef<LocalPath>, P2: AsRef<LocalPath>>(from: P, to: P2) -> Result<()> {
    let from = CString::new(from.as_ref().as_str())?;
    let to = CString::new(to.as_ref().as_str())?;

    unsafe { sys::rename(from.as_ptr(), to.as_ptr()) }.remap_errno::<FileError>()?;

    Ok(())
}
