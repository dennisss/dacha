use alloc::string::String;
use alloc::vec::Vec;
use std::borrow::ToOwned;

use common::errors::*;

use crate::{FileError, LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf};

// TODO: Better error passthrough?

#[error]
pub struct DirLockError;

/// Allows for holding an exclusive lock on a directory
///
/// This works by creating a file named 'LOCK' inside of the directory and
/// acquiring a lock with the file system on that file.
///
/// TODO: Eventually we should require that most file structs get opened using a
/// DirLock or a path derived from a single DirLock to gurantee that only one
/// struct/process has access to it
pub struct DirLock {
    /// File handle for the lock file that we create to hold the lock
    /// NOTE: Even if we don't use this, it must be held allocated to maintain
    /// the lock
    file: LocalFile,

    /// Extra reference to the directory path that we represent
    path: LocalPathBuf,
}

impl DirLock {
    /// Locks an existing directory.
    ///
    /// May return a DirLockError
    ///
    /// TODO: Support locking based on an application name which we could save
    /// in the lock file
    pub async fn open(path: &LocalPath) -> Result<DirLock> {
        if !crate::exists(path).await? {
            return Err(err_msg("Folder does not exist"));
        }

        let lockfile_path = path.join(String::from("LOCK"));

        // Before we create a lock file, verify that the directory is empty (partially
        // ensuring that all previous owners of this directory also respected the
        // locking rules)
        if !crate::exists(&lockfile_path).await? {
            let nfiles = crate::read_dir(path)?.len();
            if nfiles > 0 {
                return Err(err_msg("Folder is not empty"));
            }
        }

        let lockfile = LocalFile::open_with_options(
            lockfile_path,
            &LocalFileOpenOptions::new()
                .read(true)
                .write(true)
                .create(true),
        )
        .map_err(|_| err_msg("Failed to open the lockfile"))?;

        // Acquire the exclusive lock

        if let Err(e) = lockfile.try_lock_exclusive() {
            if let Some(FileError::LockContention) = e.downcast_ref() {
                return Err(DirLockError.into());
            }

            return Err(e);
        }

        Ok(DirLock {
            file: lockfile,
            path: path.to_owned(),
        })
    }

    pub fn path(&self) -> &LocalPath {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use crate::temp::TempDir;

    use super::*;

    #[testcase]
    async fn dir_lock_works() -> Result<()> {
        let dir = TempDir::create()?;

        let lock = DirLock::open(dir.path()).await.unwrap();

        let lock2_err = match DirLock::open(dir.path()).await {
            Err(e) => e,
            _ => panic!(),
        };
        assert!(
            lock2_err.downcast_ref::<DirLockError>().is_some(),
            "{}",
            lock2_err
        );

        drop(lock);

        // Should now succeed as we dropped the lock.
        DirLock::open(dir.path()).await.unwrap();

        Ok(())
    }
}
