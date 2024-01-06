use std::collections::HashMap;
use std::collections::HashSet;
use std::ops::DerefMut;
use std::sync::Mutex;

use common::errors::*;
use common::io::Readable;
use common::io::Writeable;
use file::LocalFile;
use file::{LocalFileOpenOptions, LocalPath, LocalPathBuf};

/// Accessor for all file paths contained within a database directory.
pub struct FilePaths {
    root_dir: LocalPathBuf,

    /// Paths to existing files in the database directory which were present
    /// before we opened the database.
    existing_files: HashMap<FileId, LocalPathBuf>,

    used_existing_files: Mutex<HashSet<FileId>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum FileId {
    Table(u64),
    Log(u64),
    Manifest(u64),
}

impl FilePaths {
    pub async fn new(root_dir: &LocalPath) -> Result<Self> {
        let mut existing_files = HashMap::new();

        for entry in file::read_dir(root_dir)? {
            let id = {
                if let Some(num) = entry.name().strip_prefix("MANIFEST-") {
                    Some(FileId::Manifest(num.parse()?))
                } else if let Some(num) = entry.name().strip_suffix(".ldb") {
                    // LevelDB table
                    Some(FileId::Table(num.parse()?))
                } else if let Some(num) = entry.name().strip_suffix(".sst") {
                    // RocksDB table
                    Some(FileId::Table(num.parse()?))
                } else if let Some(num) = entry.name().strip_suffix(".log") {
                    Some(FileId::Log(num.parse()?))
                } else {
                    // TOOD: Eventually get full coverage and log these unknown files.
                    None
                }
            };

            if let Some(id) = id {
                if existing_files.contains_key(&id) {
                    return Err(format_err!("Duplicate db file with id {:?}", id));
                }

                existing_files.insert(id, root_dir.join(entry.name()));
            }
        }

        Ok(Self {
            root_dir: root_dir.to_owned(),
            existing_files,
            used_existing_files: Mutex::new(HashSet::new()),
        })
    }

    pub fn root_dir(&self) -> &LocalPath {
        &self.root_dir
    }

    /// Empty file used to guarantee that exactly one process is accessing the
    /// DB data directory at a single time.
    ///
    /// The lock is engaged via sycalls, namely fcntl(.., F_SETLK)
    pub fn lock(&self) -> LocalPathBuf {
        self.root_dir.join("LOCK")
    }

    /// File containing the database UUID.
    ///
    /// Only present in RocksDB compatible databases. Note that RocksDB by
    /// default doesn't write the uuid to the manifest and only only writes it
    /// to this file.
    pub fn identity(&self) -> LocalPathBuf {
        self.root_dir.join("IDENTITY")
    }

    /// File that contains the filename of the currently active manifest.
    fn current(&self) -> LocalPathBuf {
        self.root_dir.join("CURRENT")
    }

    pub fn log(&self, num: u64) -> LocalPathBuf {
        if let Some(path) = self.existing_files.get(&FileId::Log(num)) {
            self.used_existing_files
                .lock()
                .unwrap()
                .insert(FileId::Log(num));
            return path.clone();
        }

        self.root_dir.join(format!("{:06}.log", num))
    }

    pub fn manifest(&self, num: u64) -> LocalPathBuf {
        if let Some(path) = self.existing_files.get(&FileId::Manifest(num)) {
            self.used_existing_files
                .lock()
                .unwrap()
                .insert(FileId::Manifest(num));
            return path.clone();
        }

        self.root_dir.join(format!("MANIFEST-{:06}", num))
    }

    pub fn table(&self, num: u64) -> LocalPathBuf {
        if let Some(path) = self.existing_files.get(&FileId::Table(num)) {
            self.used_existing_files
                .lock()
                .unwrap()
                .insert(FileId::Table(num));
            return path.clone();
        }

        self.root_dir.join(format!("{:06}.sst", num))
    }

    pub async fn current_manifest(&self) -> Result<Option<LocalPathBuf>> {
        let path = self.current();

        if !file::exists(&path).await? {
            return Ok(None);
        }

        let mut current_file = LocalFile::open(path)?;

        let mut contents = String::new();
        current_file.read_to_string(&mut contents).await?;

        let (file_name, _) = contents
            .split_once("\n")
            .ok_or_else(|| err_msg("No new line found in CURRENT file"))?;

        let path = self.root_dir.join(file_name);

        for (id, p) in &self.existing_files {
            if p == &path {
                self.used_existing_files.lock().unwrap().insert(id.clone());
                break;
            }
        }

        Ok(Some(path))
    }

    pub async fn set_current_manifest(&self, num: u64) -> Result<()> {
        let new_absolute_path = self.manifest(num);
        let mut new_path = format!("{}\n", new_absolute_path.file_name().unwrap());

        // NOTE: We intentionally do not truncate on open.
        let mut current_file = LocalFile::open_with_options(
            self.current(),
            LocalFileOpenOptions::new()
                .write(true)
                .create(true)
                .sync_on_flush(true),
        )?;

        // This should be atomic as the file name should pretty much always fit within
        // one disk sector.
        current_file.write_all(new_path.as_bytes()).await?;

        current_file.set_len(new_path.len() as u64).await?;

        current_file.flush().await?;

        Ok(())
    }

    /// Returned a list of existing file paths which haven't been reference by
    /// the program yet.
    pub fn unused_files(&self) -> Vec<LocalPathBuf> {
        let mut unused = vec![];
        let used_ids = self.used_existing_files.lock().unwrap();

        for (existing_id, path) in &self.existing_files {
            if used_ids.contains(existing_id) {
                continue;
            }

            unused.push(path.clone());
        }

        unused
    }

    pub async fn cleanup_unused_files(&self) -> Result<()> {
        let paths = self.unused_files();

        for p in paths {
            eprintln!("Deleting unused file in DB: {:?}", p);
            file::remove_file(p).await?;
        }

        Ok(())
    }

    // TODO: Eventually should we support cleaning up unknown files in the data
    // directory?
}
