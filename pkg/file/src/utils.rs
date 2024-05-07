use core::ffi::CStr;

use alloc::borrow::ToOwned;
use alloc::{ffi::CString, string::String, vec::Vec};

use common::io::Readable;
use common::{errors::*, io::Writeable};
use executor::RemapErrno;
use sys::Errno;

use crate::{
    read_dir, FileError, FileErrorKind, FileType, LocalFile, LocalFileOpenOptions, LocalPath,
    LocalPathBuf, Metadata, Permissions,
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
    let n = unsafe {
        sys::getcwd(&mut buffer).remap_errno::<FileError, _>(|| "getcwd() failed".into())?
    };

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
        .remap_errno::<FileError, _>(|| String::new())?;

    let s = String::from_utf8(buffer[..n].to_vec())?;

    Ok(LocalPathBuf::from(s))
}

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
    metadata_sync(path)
}

pub fn metadata_sync<P: AsRef<LocalPath>>(path: P) -> Result<Metadata> {
    let path = path.as_ref();
    let cpath = CString::new(path.as_str())?;
    let mut stat = sys::bindings::stat::default();
    unsafe { sys::stat(cpath.as_ptr() as *const u8, &mut stat) }
        .remap_errno::<FileError, _>(|| format!("stat(\"{}\") failed", path.as_str()))?;
    Ok(Metadata { inner: stat })
}

pub async fn symlink_metadata(path: &LocalPath) -> Result<Metadata> {
    let cpath = CString::new(path.as_str())?;
    let mut stat = sys::bindings::stat::default();
    unsafe { sys::lstat(cpath.as_ptr() as *const u8, &mut stat) }
        .remap_errno::<FileError, _>(|| format!("lstat(\"{}\") failed", path.as_str()))?;
    Ok(Metadata { inner: stat })
}

/// Creates a symlink at 'new' which points to 'old'.
pub async fn symlink<P: AsRef<LocalPath>, P2: AsRef<LocalPath>>(old: P, new: P2) -> Result<()> {
    let old = CString::new(old.as_ref().as_str())?;
    let new = CString::new(new.as_ref().as_str())?;
    unsafe { sys::symlink(old.as_ptr() as *const u8, new.as_ptr() as *const u8)? };
    Ok(())
}

/// TODO: This only applies to some operations. It might make more sense to have
/// operation specific error variants.
pub fn because_file_doesnt_exist(error: &Error) -> bool {
    if let Some(err) = error.downcast_ref::<FileError>() {
        match err.kind {
            FileErrorKind::NotFound | FileErrorKind::NotADirectory | FileErrorKind::InvalidPath => {
                true
            }
            _ => false,
        }
    } else {
        false
    }
}

/// TODO: Test that this can distinguish between a normal not_found and a
/// permission error.
pub async fn exists<P: AsRef<LocalPath>>(path: P) -> Result<bool> {
    exists_sync(path)
}

pub fn exists_sync<P: AsRef<LocalPath>>(path: P) -> Result<bool> {
    // TODO: Use symlink_metadata?
    match metadata_sync(path.as_ref()) {
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
    unsafe {
        sys::mkdir(path.as_ptr() as *const u8, 0o777)
            .remap_errno::<FileError, _>(|| String::new())?
    }
    Ok(())
}

pub async fn create_dir_all<P: AsRef<LocalPath>>(path: P) -> Result<()> {
    let mut stack = vec![];

    // We need to normalize this to ensure that every parent path is actually the
    // parent directory of the current one.
    let normalized_path = path.as_ref().normalized();

    let mut cur = Some(normalized_path.as_path());
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
    unsafe {
        sys::chmod(path.as_ptr() as *const u8, perms.mode)
            .remap_errno::<FileError, _>(|| String::new())?
    }
    Ok(())
}

pub async fn write<P: AsRef<LocalPath>, V: AsRef<[u8]>>(path: P, value: V) -> Result<()> {
    let mut file = LocalFile::open_with_options(
        path,
        &LocalFileOpenOptions::new()
            .read(false)
            .write(true)
            .create(true)
            .truncate(true),
    )?;
    file.write_all(value.as_ref()).await?;

    Ok(())
}

pub async fn append<P: AsRef<LocalPath>, V: AsRef<[u8]>>(path: P, value: V) -> Result<()> {
    let mut file = LocalFile::open_with_options(
        path,
        &LocalFileOpenOptions::new()
            .read(false)
            .write(true)
            .create(true)
            .append(true),
    )?;
    file.write_all(value.as_ref()).await?;

    Ok(())
}

pub async fn remove_dir<P: AsRef<LocalPath>>(path: P) -> Result<()> {
    let path = path.as_ref();
    let path = CString::new(path.as_str())?;

    unsafe {
        sys::rmdir(path.as_ptr() as *const u8).remap_errno::<FileError, _>(|| String::new())?
    };
    Ok(())
}

pub async fn remove_file<P: AsRef<LocalPath>>(path: P) -> Result<()> {
    let path = path.as_ref();
    let path = CString::new(path.as_str())?;

    unsafe {
        sys::unlink(path.as_ptr() as *const u8).remap_errno::<FileError, _>(|| String::new())?
    };
    Ok(())
}

/// NOTE: Only relevant for local files (not all blob based ones).
pub fn chown<P: AsRef<LocalPath>>(path: P, uid: sys::Uid, gid: sys::Gid) -> Result<()> {
    let path = path.as_ref();
    let path = CString::new(path.as_str())?;

    unsafe {
        sys::chown(path.as_ptr() as *const u8, uid, gid)
            .remap_errno::<FileError, _>(|| String::new())?
    };
    Ok(())
}

/// Deletes the contents of the directory specified by 'path' and the directory
/// itself.
///
/// - Fails if 'path' is not a directory (e.g. is a symlink to a directory).
/// - Will not delete any directories behind symlinks (will instead just delete
///   the symlink to an internal directory).
pub async fn remove_dir_all<P: AsRef<LocalPath>>(path: P) -> Result<()> {
    remove_dir_all_with_options(path.as_ref(), false).await
}

pub async fn remove_dir_all_with_options(path: &LocalPath, only_remove_dirs: bool) -> Result<()> {
    let path = path.as_ref();

    let meta = symlink_metadata(path).await?;
    if !meta.is_dir() {
        return Err(FileError::new(FileErrorKind::NotADirectory, "").into());
    }

    // List of which directories we will next process. The boolean indicates whether
    // or not we have listed the contents of the directory yet.
    let mut stack = vec![];
    stack.push((path.to_owned(), false));

    while !stack.is_empty() {
        let i = stack.len() - 1;
        let (dir, traversed) = stack[i].clone();

        let mut empty = true;
        if !traversed {
            for entry in crate::read_dir(&dir)? {
                let path = dir.join(entry.name());
                if entry.typ() == FileType::Directory {
                    empty = false;
                    stack.push((path, false));
                } else {
                    if only_remove_dirs {
                        continue;
                    }

                    remove_file(path).await?;
                }
            }
        }

        if empty {
            assert!(stack.len() == i + 1);
            remove_dir(dir).await?;
            stack.pop();
        }
    }

    Ok(())
}

/// Moves the file currently located at 'from' to 'to'
pub async fn rename<P: AsRef<LocalPath>, P2: AsRef<LocalPath>>(from: P, to: P2) -> Result<()> {
    let cfrom = CString::new(from.as_ref().as_str())?;
    let cto = CString::new(to.as_ref().as_str())?;

    unsafe { sys::rename(cfrom.as_ptr(), cto.as_ptr()) }.remap_errno::<FileError, _>(|| {
        format!(
            "rename(\"{}\", \"{}\") failed",
            from.as_ref().as_str(),
            to.as_ref().as_str()
        )
    })?;

    Ok(())
}

/// Copy a file/directory is located at 'from' to 'to' possibly recursively.
///
/// 'to' must not already exist.
pub async fn copy_all<P: AsRef<LocalPath>, P2: AsRef<LocalPath>>(from: P, to: P2) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();

    if crate::exists(to).await? {
        return Err(FileError::new(FileErrorKind::AlreadyExists, "").into());
    }

    let mut relative_paths = vec![];
    relative_paths.push(LocalPath::new("").to_owned());

    while let Some(relative_path) = relative_paths.pop() {
        let from_path = from.join(&relative_path);
        let to_path = to.join(&relative_path);

        let meta = crate::symlink_metadata(&from_path).await?;
        if meta.is_dir() {
            create_dir(&to_path).await?;

            for entry in read_dir(&from_path)? {
                relative_paths.push(relative_path.join(entry.name()));
            }
        } else if meta.is_file() {
            copy(&from_path, &to_path).await?;
        } else {
            return Err(format_err!("Can't copy {:?}", from_path));
        }
    }

    Ok(())
}

/// Copies a single regular file from 'from' to 'to'. Any existing file at 'to'
/// will be overwritten.
pub async fn copy<P: AsRef<LocalPath>, P2: AsRef<LocalPath>>(from: P, to: P2) -> Result<()> {
    let from = from.as_ref();
    let to = to.as_ref();

    let data = crate::read(from).await?;
    crate::write(to, &data[..]).await?;

    Ok(())
}
