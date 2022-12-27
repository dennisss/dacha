use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use alloc::borrow::ToOwned;
use common::errors::*;
use common::io::Writeable;

use crate::{LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf};

pub struct SyncedFile {
    file: LocalFile,

    /// Reference to the file descriptor for the directory in which this file is
    /// located.
    ///
    /// This will be present for newly opened files which haven't yet been
    /// flushed yet.
    dir: Option<Arc<LocalFile>>,
}

impl SyncedFile {
    pub async fn flush_and_sync(&mut self) -> Result<()> {
        // Flush any internal buffering in the instance to the OS.
        self.file.flush().await?;

        // fdatasync()
        self.file.sync_data().await?;

        // The first time the file is flushed, we will also flush the directory
        // containing it. This is needed for recently created files.
        if let Some(dir) = self.dir.take() {
            dir.sync_all().await?; // fsync()
        }

        Ok(())
    }
}

impl Deref for SyncedFile {
    type Target = LocalFile;

    fn deref(&self) -> &Self::Target {
        &self.file
    }
}

impl DerefMut for SyncedFile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.file
    }
}

pub struct SyncedPath {
    full_path: LocalPathBuf,
    dir: Arc<LocalFile>,
}

impl SyncedPath {
    /// NOTE: Rather than using this, it is recommended to open a
    /// SyncedDirectory and then re-use that to create many paths.
    pub async fn from(path: &LocalPath) -> Result<Self> {
        let dir = SyncedDirectory::open(
            path.parent()
                .ok_or_else(|| err_msg("Path not in a directory"))?,
        )
        .await?;

        let file_name = path
            .file_name()
            .ok_or_else(|| err_msg("Path doesn't reference a file"))?;

        dir.path(file_name)
    }

    pub async fn open(self, options: &LocalFileOpenOptions) -> Result<SyncedFile> {
        let file = LocalFile::open_with_options(self.full_path, options)?;
        Ok(SyncedFile {
            file,
            dir: Some(self.dir),
        })
    }

    /// NOTE: Because this bypasses syncronization for writes, this should only
    /// be used for reading from files.
    pub fn read_path(&self) -> &LocalPath {
        &self.full_path
    }
}

#[derive(Clone)]
pub struct SyncedDirectory {
    file: Arc<LocalFile>,
    path: LocalPathBuf,
}

impl SyncedDirectory {
    /// Gets a reference to an existing directory on the file system.
    ///
    /// This ensures that all parent directories are fully synced to disk such
    /// that this directory can be considered durable.
    ///
    /// The childmost directory itself isn't synced.
    pub async fn open(path: &LocalPath) -> Result<Self> {
        let path = if path.is_absolute() {
            path.to_owned()
        } else {
            crate::current_dir()?.join(path)
        };

        let file = Arc::new(LocalFile::open(&path)?);

        let mut parent = path.as_path();
        while let Some(path) = parent.parent() {
            let file = LocalFile::open(path)?;
            file.sync_all().await?; // fsync

            parent = path;
        }

        Ok(Self { file, path })
    }

    pub fn path<P: AsRef<LocalPath>>(&self, relative_path: P) -> Result<SyncedPath> {
        self.path_impl(relative_path.as_ref())
    }

    fn path_impl(&self, relative_path: &LocalPath) -> Result<SyncedPath> {
        if relative_path.parent() != Some(LocalPath::new("")) {
            return Err(err_msg("Opening nested files isn't supported"));
        }

        Ok(SyncedPath {
            full_path: self.path.join(relative_path),
            dir: self.file.clone(),
        })
    }
}
