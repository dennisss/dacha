use std::ops::DerefMut;

use crate::common::io::Writeable;
use common::errors::*;
use common::futures::AsyncReadExt;
use common::io::Readable;
use file::sync::{SyncedDirectory, SyncedPath};
use file::{LocalFileOpenOptions, LocalPath};

/// Accessor for all file paths contained within a database directory.
#[derive(Clone)]
pub struct FilePaths {
    root_dir: SyncedDirectory,
}

impl FilePaths {
    pub async fn new(root_dir: &LocalPath) -> Result<Self> {
        Ok(Self {
            root_dir: SyncedDirectory::open(root_dir).await?,
        })
    }
    /// Empty file used to guarantee that exactly one process is accessing the
    /// DB data directory at a single time.
    ///
    /// The lock is engaged via sycalls, namely fcntl(.., F_SETLK)
    pub fn lock(&self) -> SyncedPath {
        self.root_dir.path("LOCK").unwrap()
    }

    /// File containing the database UUID.
    ///
    /// Only present in RocksDB compatible databases. Note that RocksDB by
    /// default doesn't write the uuid to the manifest and only only writes it
    /// to this file.
    pub fn identity(&self) -> SyncedPath {
        self.root_dir.path("IDENTITY").unwrap()
    }

    /// File that contains the filename of the currently active manifest.
    fn current(&self) -> SyncedPath {
        self.root_dir.path("CURRENT").unwrap()
    }

    pub fn log(&self, num: u64) -> SyncedPath {
        self.root_dir.path(format!("{:06}.log", num)).unwrap()
    }

    pub fn manifest(&self, num: u64) -> SyncedPath {
        self.root_dir.path(format!("MANIFEST-{:06}", num)).unwrap()
    }

    pub fn table(&self, num: u64) -> SyncedPath {
        self.root_dir.path(format!("{:06}.ldb", num)).unwrap()
    }

    pub async fn current_manifest(&self) -> Result<Option<SyncedPath>> {
        let path = self.current();

        // TODO: Exists may ignore errors such as permission errors.
        if !file::exists(path.read_path()).await? {
            return Ok(None);
        }

        let mut current_file = path.open(&LocalFileOpenOptions::new().read(true)).await?;

        let mut contents = String::new();
        current_file.read_to_string(&mut contents).await?;

        let (file_name, _) = contents
            .split_once("\n")
            .ok_or_else(|| err_msg("No new line found in CURRENT file"))?;

        Ok(Some(self.root_dir.path(file_name)?))
    }

    pub async fn set_current_manifest(&self, num: u64) -> Result<()> {
        // TODO: Deduplicate with .manifest().
        let new_path = format!("MANIFEST-{:06}\n", num);

        // NOTE: We intentionally do not truncate on open.
        let mut current_file = self
            .current()
            .open(&LocalFileOpenOptions::new().write(true).create(true))
            .await?;

        // This should be atomic as the file name should pretty much always fit within
        // one disk sector.
        current_file.write_all(new_path.as_bytes()).await?;

        current_file.set_len(new_path.len() as u64).await?;

        current_file.flush_and_sync().await?;

        Ok(())
    }

    // TODO: Eventually should we support cleaning up unknown files in the data
    // directory?
}
