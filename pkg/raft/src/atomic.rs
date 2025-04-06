use common::bytes::Bytes;
use common::errors::*;
use common::io::{Readable, Writeable};
use file::{LocalFile, LocalFileOpenOptions, LocalPath, LocalPathBuf};

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

// TODO: open must distinguish between failing to read existing data and failing
// because it doesn't exist

// https://docs.rs/libc/0.2.48/libc/fn.unlinkat.html
// TODO: Also linux's rename will atomically replace any overriden file so we
// could use this fact to remove one more syscall from the process


/// Wraps a binary blob that can be atomically read/written from the disk.
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
}

impl BlobFile {
    // TODO: If I wanted to be super Rusty, I could represent whether or not it
    // exists (i.e. whether create() or open() should be called) by returning an
    // enum here instead of relying on the user checking the value of exists()
    // at runtime
    pub async fn builder(path: &LocalPath) -> Result<BlobFileBuilder> {
        let path = path.to_owned();
        let path_tmp = LocalPathBuf::from(&(path.as_str().to_owned() + ".tmp"));

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
        // Write new data to a '.tmp' file
        {
            let mut f = LocalFile::open_with_options(
                &self.path_tmp, LocalFileOpenOptions::new().create(true).truncate(true).write(true))?;
            f.write_all(data).await?;
            f.sync_data().await?;
        }

        // Rename to the regular path (on Linux this atomically replaces any old file).
        // TODO: Dedup all the code that takes advance of this atomic property.
        {
            // https://man7.org/linux/man-pages/man2/rename.2.html
            // TODO: Directly reference the syscall to ensure this is atomic.
            file::rename(&self.path_tmp, &self.path).await?;
        }

        // Sync the directory to make it permanent.
        // (TODO: Need to ensure we sync the whole directory chain up to the root of the fs).
        self.dir.sync_data().await?;

        Ok(())
    }
}

pub struct BlobFileBuilder {
    inner: BlobFile,
}

impl BlobFileBuilder {
    pub async fn exists(&self) -> Result<bool> {
        file::exists(&self.inner.path).await
    }

    /// If any existing data exists, this will delete it
    pub async fn purge(&self) -> Result<()> {
        if file::exists(&self.inner.path).await? {
            file::remove_file(&self.inner.path).await?;
        }

        if file::exists(&self.inner.path_tmp).await? {
            file::remove_file(&self.inner.path_tmp).await?;
        }

        Ok(())
    }

    /// Opens the file assuming that it exists
    /// Errors out if we could be not read the data because it is non-existent
    pub async fn open(self) -> Result<(BlobFile, Bytes)> {
        if !self.exists().await? {
            return Err(err_msg("File does not exist"));
        }

        let inst = self.inner;
        let data = file::read(&inst.path).await?.into();
        Ok((inst, data))
    }

    /// Creates a new file with the given initial value
    /// Errors out if any data already exists or if the write fails
    pub async fn create(self, initial_value: &[u8]) -> Result<BlobFile> {
        if self.exists().await? {
            return Err(err_msg("Existing data already exists"));
        }

        let mut inst = self.inner;
        inst.store(initial_value).await?;

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
