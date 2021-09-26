use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use common::async_std::fs::{self, File, OpenOptions};
use common::async_std::io::prelude::*;
use common::async_std::io::SeekFrom;
use common::async_std::path::{Path, PathBuf};
use common::bytes::Bytes;
use common::errors::*;
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;

/// Amount of padding that we add to the file for the length and checksume bytes
const PADDING: u64 = 8;

const DISK_SECTOR_SIZE: u64 = 512;

/*
    Cases to test:
    - Upon a failed creation, we should not report the file as created
        - Successive calls to create() should be able to delete any partially created state

*/

/*
    The read index in the case of the memory store
    -> Important notice

*/

/*
    NOTE: etcd/raft assumes that the entire snapshot fits in memory
    -> Not particularly good
    -> Fine as long as limit range sizes for
*/

// Simple case is to just generate a callback

// https://docs.rs/libc/0.2.48/libc/fn.unlinkat.html
// TODO: Also linux's rename will atomically replace any overriden file so we
// could use this fact to remove one more syscall from the process

/// Wraps a binary blob that can be atomically read/written from the disk
/// Additionally this will add some checksumming to the file to verify the
/// integrity of the data and accept/reject partial reads
///
/// NOTE: If any operation fails, then this struct should be considered poisoned
/// and unuseable
///
/// NOTE: This struct does not deal with maintaining an internal buffer of the
/// current value, so that is someone elses problem as this is meant to be super
/// light weight
///
/// NOTE: This assumes that this object is being given exclusive access to the
/// given path (meaning that the directory is locked)
pub struct BlobFile {
    // TODO: Would also be good to know the size of it
    /// Cached open file handle to the directory containing the file
    dir: File,

    /// The path to the main data file this uses
    path: PathBuf,

    /// Path to temporary data file used to store the old data value until the
    /// new value is fully written
    path_tmp: PathBuf,

    /// Path to a temporary file used only during initial creation of the file
    /// It will only exist if the file has never been successfully created
    /// before.
    path_new: PathBuf,
}

pub struct BlobFileBuilder {
    inner: BlobFile,
}

// TODO: For unlinks, unlinkat would probably be most efficient using a relative
// path
// XXX: Additionally openat for

// Writing will always create a new file right?

// TODO: open must distinguish between failing to read existing data and failing
// because it doesn't exist

impl BlobFile {
    // TODO: If I wanted to be super Rusty, I could represent whether or not it
    // exists (i.e. whether create() or open() should be called) by returning an
    // enum here instead of relying on the user checking the value of exists()
    // at runtime
    pub async fn builder(path: &Path) -> Result<BlobFileBuilder> {
        let path = path.to_owned();
        let path_tmp = PathBuf::from(&(path.to_str().unwrap().to_owned() + ".tmp"));
        let path_new = PathBuf::from(&(path.to_str().unwrap().to_owned() + ".new"));

        let dir = {
            let path_dir = match path.parent() {
                Some(p) => p,
                None => return Err(err_msg("Path is not in a directory")),
            };

            if !path_dir.exists().await {
                return Err(err_msg("Directory does not exist"));
            }

            File::open(&path_dir).await?
        };

        Ok(BlobFileBuilder {
            inner: BlobFile {
                dir,
                path,
                path_tmp,
                path_new,
            },
        })
    }

    /// Overwrites the file with a new value (atomically of course)
    ///
    /// TODO: Switch to using the SyncedPath system.
    pub async fn store(&self, data: &[u8]) -> Result<()> {
        let new_filesize = (data.len() as u64) + PADDING;

        // Performant case of usually only requiring one sector write to replace the
        // file (aside from possibly needing to change the length of it)
        // TODO: We could just make sure that these files are always at least 512bytes
        // in length in order to avoid having to do all of the truncation and length
        // changes TODO: Possibly speed up by caching the size of the old file?
        if new_filesize < DISK_SECTOR_SIZE {
            let old_filesize = self.path.metadata().await?.len();

            let mut file = OpenOptions::new().write(true).open(&self.path).await?;

            if new_filesize > old_filesize {
                file.set_len(new_filesize).await?;
                file.sync_data().await?;
            }

            Self::write_simple(&mut file, data).await?;

            if new_filesize < old_filesize {
                file.set_len(new_filesize).await?;
            }

            file.sync_data().await?;

            return Ok(());
        }

        // Rename old value
        fs::rename(&self.path, &self.path_tmp).await?;
        self.dir.sync_data().await?;

        // Write new value
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)
            .await?;
        Self::write_simple(&mut file, data).await?;
        file.sync_data().await?;
        self.dir.sync_data().await?;

        // Remove old value
        /*
        // Basically must cache the actual file name
        {
            if cfg!(any(target_os = "linux")) {
                let ret = unsafe {
                    libc::fallocate(file.as_raw_fd(), libc::FALLOC_FL_KEEP_SIZE, 0, len as libc::off_t)
                };

                if ret == 0 { Ok(()) } else { Err(Error::last_os_error()) }
            }
            else {

            }
        }
        */

        fs::remove_file(&self.path_tmp).await?;

        // NOTE: A dir sync should not by needed here

        Ok(())
    }

    async fn write_simple(file: &mut File, data: &[u8]) -> Result<u64> {
        let sum = {
            let mut hasher = CRC32CHasher::new();
            hasher.update(data);
            hasher.finish_u32()
        };

        file.seek(SeekFrom::Start(0)).await?;

        file.write_all(&(data.len() as u32).to_le_bytes()).await?;
        file.write_all(data).await?;
        file.write_all(&(sum as u32).to_le_bytes()).await?;

        let pos = file.seek(SeekFrom::Current(0)).await?;
        assert_eq!(pos, (data.len() as u64) + PADDING);

        Ok(pos)
    }
}

impl BlobFileBuilder {
    pub async fn exists(&self) -> bool {
        self.inner.path.exists().await || self.inner.path_tmp.exists().await
    }

    /// If any existing data exists, this will delete it
    pub async fn purge(&self) -> Result<()> {
        if self.inner.path.exists().await {
            fs::remove_file(&self.inner.path).await?;
        }

        if self.inner.path_tmp.exists().await {
            fs::remove_file(&self.inner.path_tmp).await?;
        }

        if self.inner.path_new.exists().await {
            fs::remove_file(&self.inner.path_new).await?;
        }

        Ok(())
    }

    /// Opens the file assuming that it exists
    /// Errors out if we could be not read the data because it is corrupt or
    /// non-existent
    pub async fn open(self) -> Result<(BlobFile, Bytes)> {
        if !self.exists().await {
            return Err(err_msg("File does not exist"));
        }

        let inst = self.inner;

        if inst.path.exists().await {
            let res = Self::try_open(&inst.path).await?;
            if let Some(data) = res {
                if inst.path_tmp.exists().await {
                    fs::remove_file(&inst.path_tmp).await?;
                    inst.dir.sync_all().await?;
                }

                return Ok((inst, data));
            }
        }

        if inst.path_tmp.exists().await {
            let res = Self::try_open(&inst.path_tmp).await?;
            if let Some(data) = res {
                if inst.path.exists().await {
                    fs::remove_file(&inst.path).await?;
                }

                fs::rename(&inst.path_tmp, &inst.path).await?;
                inst.dir.sync_all().await?;

                return Ok((inst, data));
            }
        }

        Err(err_msg("No valid data could be read (corrupt data)"))
    }

    /// Tries to open the given path
    /// Returns None if the file doesn't contain valid data in it
    async fn try_open(path: &Path) -> Result<Option<Bytes>> {
        let mut file = File::open(path).await?;

        let mut buf = vec![];
        file.read_to_end(&mut buf).await?;

        let length = {
            if buf.len() < 4 {
                return Ok(None);
            }
            (&buf[0..4]).read_u32::<LittleEndian>()? as usize
        };

        let data_start = 4;
        let data_end = data_start + length;
        let checksum_end = data_end + 4;

        assert_eq!(checksum_end, length + (PADDING as usize));

        if buf.len() < checksum_end {
            return Ok(None);
        }

        let sum = {
            let mut hasher = CRC32CHasher::new();
            hasher.update(&buf[data_start..data_end]);
            hasher.finish_u32()
        };
        let expected_sum = (&buf[data_end..checksum_end]).read_u32::<LittleEndian>()?;

        if sum != expected_sum {
            return Ok(None);
        }

        // File is larger than its valid contents (we will just truncate it)
        if buf.len() > checksum_end {
            file.set_len(data_end as u64).await?;
        }

        let bytes = Bytes::from(buf);

        Ok(Some(bytes.slice(data_start..data_end)))
    }

    /// Creates a new file with the given initial value
    /// Errors out if any data already exists or if the write fails
    pub async fn create(self, initial_value: &[u8]) -> Result<BlobFile> {
        if self.exists().await {
            return Err(err_msg("Existing data already exists"));
        }

        let inst = self.inner;

        // This may occur if we previously tried creating a data file but we
        // were never able to suceed
        if inst.path_new.exists().await {
            fs::remove_file(&inst.path_new).await?;
        }

        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);

        let mut file = opts.open(&inst.path_new).await?;

        BlobFile::write_simple(&mut file, initial_value).await?;
        file.sync_all().await?;

        fs::rename(&inst.path_new, &inst.path).await?;

        inst.dir.sync_all().await?;

        Ok(inst)
    }
}
