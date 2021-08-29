use std::path::{Path, PathBuf};

/// Accessor for all file paths contained within a database directory.
pub struct FilePaths {
    root_dir: PathBuf,
}

impl FilePaths {
    pub fn new(root_dir: PathBuf) -> Self {
        Self { root_dir }
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    /// Empty file used to guarantee that exactly one process is accessing the
    /// DB data directory at a single time.
    ///
    /// The lock is engaged via sycalls, namely fcntl(.., F_SETLK)
    pub fn lock(&self) -> PathBuf {
        self.root_dir.join("LOCK")
    }

    /// File containing the database UUID.
    ///
    /// Only present in RocksDB compatible databases. Note that RocksDB by
    /// default doesn't write the uuid to the manifest and only only writes it
    /// to this file.
    pub fn identity(&self) -> PathBuf {
        self.root_dir.join("IDENTITY")
    }

    /// File that contains the filename of the currently active manifest.
    pub fn current(&self) -> PathBuf {
        self.root_dir.join("CURRENT")
    }

    pub fn log(&self, num: u64) -> PathBuf {
        self.root_dir.join(format!("{:06}.log", num))
    }

    pub fn manifest(&self, num: u64) -> PathBuf {
        self.root_dir.join(format!("MANIFEST-{:06}", num))
    }

    // TODO: Eventually should we support cleaning up unknown files in the data
    // directory?
}
