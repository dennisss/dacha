use common::bytes::Bytes;
use common::errors::*;
use common::io::{Readable, Writeable};
use crypto::checksum::crc::CRC32CHasher;
use crypto::hasher::Hasher;
use file::{LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf};

/// Amount of padding that we add to the file for the length and checksum bytes
const PADDING: u64 = 8;

const DISK_SECTOR_SIZE: u64 = 512;

/*
    Cases to test:
    - Upon a failed creation, we should not report the file as created
        - Successive calls to create() should be able to delete any partially created state
*/
/*
    NOTE: etcd/raft assumes that the entire snapshot fits in memory
    -> Not particularly good
    -> Fine as long as limit range sizes for

    TODO: ALso sync the directories leading up to the file.
*/

// TODO: For unlinks, unlinkat would probably be most efficient using a relative
// path
// XXX: Additionally openat for

// Writing will always create a new file right?

// TODO: open must distinguish between failing to read existing data and failing
// because it doesn't exist

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
    dir: LocalFile,

    /// The path to the main data file this uses
    path: LocalPathBuf,

    /// Path to temporary data file used to store the old data value until the
    /// new value is fully written
    path_tmp: LocalPathBuf,

    /// Path to a temporary file used only during initial creation of the file
    /// It will only exist if the file has never been successfully created
    /// before.
    path_new: LocalPathBuf,
}

impl BlobFile {
    // TODO: If I wanted to be super Rusty, I could represent whether or not it
    // exists (i.e. whether create() or open() should be called) by returning an
    // enum here instead of relying on the user checking the value of exists()
    // at runtime
    pub async fn builder(path: &LocalPath) -> Result<BlobFileBuilder> {
        let path = path.to_owned();
        let path_tmp = LocalPathBuf::from(&(path.as_str().to_owned() + ".tmp"));
        let path_new = LocalPathBuf::from(&(path.as_str().to_owned() + ".new"));

        // TODO: Should sync all parent directories of this directory.
        let dir = {
            let path_dir = match path.parent() {
                Some(p) => p,
                None => return Err(err_msg("Path is not in a directory")),
            };

            if !file::exists(path_dir).await? {
                return Err(err_msg("Directory does not exist"));
            }

            LocalFile::open(&path_dir)?
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

    /// Overwrites the file with a new value (atomically of course).
    ///
    /// NOTE: This intentionally requires mutable access to the BlobFile
    /// instance since concurrent writes are not supported.
    ///
    /// TODO: Switch to using the SyncedPath system.
    pub async fn store(&mut self, data: &[u8]) -> Result<()> {
        let new_filesize = (data.len() as u64) + PADDING;

        // Performant case of usually only requiring one sector write to replace the
        // file (aside from possibly needing to change the length of it)
        // TODO: We could just make sure that these files are always at least 512bytes
        // in length in order to avoid having to do all of the truncation and length
        // changes TODO: Possibly speed up by caching the size of the old file?
        if new_filesize < DISK_SECTOR_SIZE {
            let old_filesize = file::metadata(&self.path).await?.len();

            let mut file =
                LocalFile::open_with_options(&self.path, LocalFileOpenOptions::new().write(true))?;

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
        file::rename(&self.path, &self.path_tmp).await?;
        self.dir.sync_data().await?;

        // Write new value
        let mut file = LocalFile::open_with_options(
            &self.path,
            LocalFileOpenOptions::new().write(true).create_new(true),
        )?;
        Self::write_simple(&mut file, data).await?;
        file.flush().await?;
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

        file::remove_file(&self.path_tmp).await?;

        // NOTE: A dir sync should not by needed here

        Ok(())
    }

    async fn write_simple(file: &mut LocalFile, data: &[u8]) -> Result<u64> {
        let sum = {
            let mut hasher = CRC32CHasher::new();
            hasher.update(data);
            hasher.finish_u32()
        };

        file.seek(0);

        file.write_all(&(data.len() as u32).to_le_bytes()).await?;
        file.write_all(data).await?;
        file.write_all(&(sum as u32).to_le_bytes()).await?;

        let pos = file.current_position();
        assert_eq!(pos, (data.len() as u64) + PADDING);

        Ok(pos)
    }
}

pub struct BlobFileBuilder {
    inner: BlobFile,
}

impl BlobFileBuilder {
    pub async fn exists(&self) -> Result<bool> {
        Ok(file::exists(&self.inner.path).await? || file::exists(&self.inner.path_tmp).await?)
    }

    /// If any existing data exists, this will delete it
    pub async fn purge(&self) -> Result<()> {
        if file::exists(&self.inner.path).await? {
            file::remove_file(&self.inner.path).await?;
        }

        if file::exists(&self.inner.path_tmp).await? {
            file::remove_file(&self.inner.path_tmp).await?;
        }

        if file::exists(&self.inner.path_new).await? {
            file::remove_file(&self.inner.path_new).await?;
        }

        Ok(())
    }

    /// Opens the file assuming that it exists
    /// Errors out if we could be not read the data because it is corrupt or
    /// non-existent
    pub async fn open(self) -> Result<(BlobFile, Bytes)> {
        if !self.exists().await? {
            return Err(err_msg("File does not exist"));
        }

        let inst = self.inner;

        if file::exists(&inst.path).await? {
            let res = Self::try_open(&inst.path).await?;
            if let Some(data) = res {
                if file::exists(&inst.path_tmp).await? {
                    file::remove_file(&inst.path_tmp).await?;
                    inst.dir.sync_all().await?;
                }

                return Ok((inst, data));
            }
        }

        if file::exists(&inst.path_tmp).await? {
            let res = Self::try_open(&inst.path_tmp).await?;
            if let Some(data) = res {
                if file::exists(&inst.path).await? {
                    file::remove_file(&inst.path).await?;
                }

                file::rename(&inst.path_tmp, &inst.path).await?;
                inst.dir.sync_all().await?;

                return Ok((inst, data));
            }
        }

        Err(err_msg("No valid data could be read (corrupt data)"))
    }

    /// Tries to open the given path
    /// Returns None if the file doesn't contain valid data in it
    async fn try_open(path: &LocalPath) -> Result<Option<Bytes>> {
        let mut file = LocalFile::open(path)?;

        let mut buf = vec![];
        file.read_to_end(&mut buf).await?;

        let length = {
            if buf.len() < 4 {
                return Ok(None);
            }
            u32::from_le_bytes(*array_ref![buf, 0, 4]) as usize
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
        let expected_sum = u32::from_le_bytes(*array_ref![buf, data_end, 4]);

        if sum != expected_sum {
            return Ok(None);
        }

        // File is larger than its valid contents (we will just truncate it)
        if buf.len() > checksum_end {
            file.set_len(checksum_end as u64).await?;
        }

        let bytes = Bytes::from(buf);

        Ok(Some(bytes.slice(data_start..data_end)))
    }

    /// Creates a new file with the given initial value
    /// Errors out if any data already exists or if the write fails
    pub async fn create(self, initial_value: &[u8]) -> Result<BlobFile> {
        if self.exists().await? {
            return Err(err_msg("Existing data already exists"));
        }

        let inst = self.inner;

        // This may occur if we previously tried creating a data file but we
        // were never able to suceed
        if file::exists(&inst.path_new).await? {
            file::remove_file(&inst.path_new).await?;
        }

        let mut file = LocalFile::open_with_options(
            &inst.path_new,
            LocalFileOpenOptions::new().write(true).create_new(true),
        )?;

        BlobFile::write_simple(&mut file, initial_value).await?;
        file.sync_all().await?;

        file::rename(&inst.path_new, &inst.path).await?;

        inst.dir.sync_all().await?;

        Ok(inst)
    }
}

#[cfg(test)]
mod tests {

    use file::temp::TempDir;

    use super::*;

    #[testcase]
    async fn blob_file_works() -> Result<()> {
        let dir = TempDir::create()?;
        let path = dir.path().join("file");

        let blob = BlobFile::builder(&path)
            .await?
            .create(b"hello_world")
            .await?;

        drop(blob);

        let (mut blob, data) = BlobFile::builder(&path).await?.open().await?;
        assert_eq!(&data[..], &b"hello_world"[..]);

        blob.store(b"new").await?;

        drop(blob);

        let (mut blob, data) = BlobFile::builder(&path).await?.open().await?;
        assert_eq!(&data[..], &b"new"[..]);

        let mut large_data = vec![0u8; 16000];
        large_data[15000] = 0xAB;

        blob.store(&large_data).await?;

        drop(blob);

        let (mut blob, data) = BlobFile::builder(&path).await?.open().await?;
        assert_eq!(&data[..], &large_data[..]);

        Ok(())
    }

    // TODO: Test large values that require renames.

    // TODO: Test various failure cases.
    // - Ideally fuzz test with random failures.
    // - Also fuzz test with very large or small values.
}
