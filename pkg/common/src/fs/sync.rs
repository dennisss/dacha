use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use crate::errors::*;
use async_std::fs::File;
use async_std::fs::OpenOptions;
use async_std::path::{Path, PathBuf};
use async_std::prelude::*;

pub struct SyncedFile {
    file: File,

    /// Reference to the file descriptor for the directory in which this file is
    /// located.
    ///
    /// This will be present for newly opened files which haven't yet been
    /// flushed yet.
    dir: Option<Arc<File>>,
}

impl SyncedFile {
    pub async fn flush_and_sync(&mut self) -> Result<()> {
        // Flush async-std internal buffer to the OS.
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
    type Target = File;

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
    full_path: PathBuf,
    dir: Arc<File>,
}

impl SyncedPath {
    /// NOTE: Rather than using this, it is recommended to open a
    /// SyncedDirectory and then re-use that to create many paths.
    pub fn from(path: &Path) -> Result<Self> {
        let dir = SyncedDirectory::open(
            path.parent()
                .ok_or_else(|| err_msg("Path not in a directory"))?,
        )?;

        let file_name = path
            .file_name()
            .ok_or_else(|| err_msg("Path doesn't reference a file"))?;

        dir.path(file_name)
    }

    pub async fn open(self, options: &OpenOptions) -> Result<SyncedFile> {
        let file = options.open(self.full_path).await?;
        Ok(SyncedFile {
            file,
            dir: Some(self.dir),
        })
    }

    /// NOTE: Because this bypasses syncronization for writes, this should only
    /// be used for reading from files.
    pub fn read_path(&self) -> &Path {
        &self.full_path
    }
}

pub struct SyncedDirectory {
    file: Arc<File>,
    path: PathBuf,
}

impl SyncedDirectory {
    /// Gets a reference to an existing directory on the file system.
    ///
    /// This ensures that all parent directories are fully synced to disk such
    /// that this directory can be considered durable.
    ///
    /// The childmost directory itself isn't synced.
    pub fn open(path: &Path) -> Result<Self> {
        let file = Arc::new(std::fs::File::open(path)?.into());

        let mut parent = path;
        while let Some(path) = parent.parent() {
            let file = std::fs::File::open(path)?;
            file.sync_all()?; // fsync

            parent = path;
        }

        Ok(Self {
            file,
            path: path.into(),
        })
    }

    pub fn path<P: AsRef<Path>>(&self, relative_path: P) -> Result<SyncedPath> {
        self.path_impl(relative_path.as_ref())
    }

    fn path_impl(&self, relative_path: &Path) -> Result<SyncedPath> {
        if relative_path.parent() != Some(Path::new("")) {
            return Err(err_msg("Opening nested files isn't supported"));
        }

        Ok(SyncedPath {
            full_path: self.path.join(relative_path),
            dir: self.file.clone(),
        })
    }
}
