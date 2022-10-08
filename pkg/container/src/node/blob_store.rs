use std::os::unix::fs::PermissionsExt;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use builder::proto::bundle::{BlobFormat, BlobSpec};
use common::async_std::fs;
use common::async_std::fs::{File, OpenOptions};
use common::async_std::path::Path;
use common::async_std::path::PathBuf;
use common::async_std::prelude::*;
use common::async_std::sync::Mutex;
use common::async_std::task;
use common::errors::*;
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use sstable::EmbeddedDB;

use crate::node::workers_table::*;
use crate::proto::blob::*;

// A blob id is an 'algorithm:lowercase_hex_digest'
// e.g. 'sha256:012345789ab...'
regexp!(BLOB_ID_PATTERN => "^([a-z0-9]+):([0-9a-f]+)$");

/// Must fit in a linux file name.
const BLOB_ID_MAX_LENGTH: usize = 255;

/// Error produced while trying to read a blob.
pub enum ReadBlobError {
    /// The blob doesn't exist locally.
    NotFound,

    /// The blob is currently exclusively locked by a writer (the blob might
    /// soon exist).
    BeingWritten,
}

/// Error produced while trying to start writing a brand new blob.
pub enum NewBlobError {
    /// The provided blob id is invalid.
    InvalidBlobId,

    /// The provided blob spec is invalid.
    InvalidBlobSpec,

    /// The blob has already been fully written to storage.
    /// (blobs are immutable when present)
    AlreadyExists,

    /// The blob is currently exclusively locked by a writer (the blob might
    /// soon exist).
    BeingWritten,
}

#[derive(Clone)]
pub struct BlobStore {
    shared: Arc<Shared>,
}

struct Shared {
    /// Directory in which we will store the raw blob data.
    ///
    /// In this directory, the following files will be stored:
    /// - './{BLOB_ID}/raw' : Raw version of the blob as uploaded.
    /// - './{BLOB_ID}/extracted/' : Directory which contains all of the files
    ///
    /// TODO: Consider acquiring a lock to this directory through the FS (or
    /// having some way to see if there are any locks to parent directories).
    dir: PathBuf,

    /// Local database used for storing blob metadata.
    db: Arc<EmbeddedDB>,

    state: Mutex<State>,
}

struct State {
    /// Set of all blobs stored on the current server.
    /// This is basically a cached copy of what is in the DB.
    ///
    /// Additionally this contains entries for blobs that are currently being
    /// created (or deleted) with exists=false.
    blobs: HashMap<String, BlobEntry>,
}

struct BlobEntry {
    spec: BlobSpec,

    /// Whether or not this blob has been written to disk fully yet. Will be
    /// false for placeholder entries used for writing blobs.
    exists: bool,

    ref_count: usize,

    exclusive_lock: bool,
}

impl BlobStore {
    /// NOTE: It is unsafe to create mutliple BlobStore instances with the same
    /// 'db' or 'dir' as they will overwrite each other's data.
    pub async fn create(dir: PathBuf, db: Arc<EmbeddedDB>) -> Result<Self> {
        let blobs = get_blob_specs(db.as_ref())
            .await?
            .into_iter()
            .map(|spec| {
                (
                    spec.id().to_string(),
                    BlobEntry {
                        spec,
                        exists: true,
                        ref_count: 0,
                        exclusive_lock: false,
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        Ok(Self {
            shared: Arc::new(Shared {
                dir,
                db,
                state: Mutex::new(State { blobs }),
            }),
        })
    }

    /// Looks up a blob in storage and acquires a reader lock/lease on it.
    /// While the returned lease is alive, the caller can read the contents of
    /// the blob.
    pub async fn read_lease(&self, blob_id: &str) -> std::result::Result<BlobLease, ReadBlobError> {
        let mut state = self.shared.state.lock().await;
        let blob = match state.blobs.get_mut(blob_id) {
            Some(v) => v,
            None => {
                return Err(ReadBlobError::NotFound);
            }
        };

        if blob.exclusive_lock {
            return Err(ReadBlobError::BeingWritten);
        }

        blob.ref_count += 1;

        Ok(BlobLease {
            shared: self.shared.clone(),
            spec: blob.spec.clone(),
        })
    }

    /// Gets a writer instance for inserting a new non-existent blob into
    /// storage. While the writer is live, no other readers/writers will
    /// exist for this blob.
    pub async fn new_writer(
        &self,
        spec: &BlobSpec,
    ) -> Result<std::result::Result<BlobWriter, NewBlobError>> {
        if spec.id().len() > BLOB_ID_MAX_LENGTH || !BLOB_ID_PATTERN.test(spec.id()) {
            return Ok(Err(NewBlobError::InvalidBlobId));
        }

        if spec.format() == BlobFormat::UNKNOWN {
            return Ok(Err(NewBlobError::InvalidBlobSpec));
        }

        let blob_id = spec.id();

        let lease = {
            let mut state = self.shared.state.lock().await;
            if let Some(existing_entry) = state.blobs.get(blob_id) {
                if existing_entry.exclusive_lock {
                    return Ok(Err(NewBlobError::BeingWritten));
                }

                return Ok(Err(NewBlobError::AlreadyExists));
            }

            state.blobs.insert(
                blob_id.to_string(),
                BlobEntry {
                    spec: spec.clone(),
                    exclusive_lock: true,
                    exists: false,
                    ref_count: 1,
                },
            );

            BlobLease {
                spec: spec.clone(),
                shared: self.shared.clone(),
            }
        };

        // Create the blob dir.
        // If the directory already exists, then likely a previous attempt to upload
        // failed, so we'll just retry.
        let blob_dir = self.shared.dir.join(blob_id);
        if !blob_dir.exists().await {
            fs::create_dir_all(&blob_dir).await?;
        }

        let mut raw_file_path = lease.raw_path();

        let mut raw_file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&raw_file_path)
            .await?;

        let mut hasher = crypto::sha256::SHA256Hasher::default();

        Ok(Ok(BlobWriter {
            lease,
            raw_file,
            hasher,
            bytes_written: 0,
        }))
    }

    async fn upload_impl<'a>(
        &self,
        mut request: rpc::ServerStreamRequest<BlobData>,
        response: &mut rpc::ServerResponse<'a, google::proto::empty::Empty>,
    ) -> Result<()> {
        let first_part = request.recv().await?.ok_or_else(|| {
            rpc::Status::invalid_argument("Expected at least one request message")
        })?;

        let mut writer = match self.new_writer(first_part.spec()).await? {
            Ok(v) => v,
            Err(NewBlobError::AlreadyExists) => {
                return Err(rpc::Status::already_exists("Blob already exists").into());
            }
            Err(NewBlobError::BeingWritten) => {
                return Err(rpc::Status::already_exists("Blob current being written").into());
            }
            Err(NewBlobError::InvalidBlobId) => {
                return Err(rpc::Status::invalid_argument("Invalid blob id").into());
            }
            Err(NewBlobError::InvalidBlobSpec) => {
                return Err(rpc::Status::invalid_argument("Invalid blob spec").into());
            }
        };

        writer.write(first_part.data()).await?;

        while let Some(part) = request.recv().await? {
            writer.write(part.data()).await?;
        }

        writer.finish().await?;

        Ok(())
    }

    /// Implementation of the Download RPC.
    async fn download_impl<'a>(
        &self,
        request: rpc::ServerRequest<BlobDownloadRequest>,
        response: &mut rpc::ServerStreamResponse<'a, BlobData>,
    ) -> Result<()> {
        let blob_id = request.blob_id();

        let lease = match self.read_lease(blob_id).await {
            Ok(v) => v,
            Err(ReadBlobError::BeingWritten) => {
                return Err(
                    rpc::Status::failed_precondition("Can't acquire reader lock to blob.").into(),
                );
            }
            Err(ReadBlobError::NotFound) => {
                return Err(rpc::Status::not_found("No blob with the given id").into());
            }
        };

        let mut first_part = BlobData::default();
        first_part.set_spec(lease.spec().clone());
        response.send(first_part).await?;

        let raw_file_path = lease.raw_path();
        let mut raw_file = fs::File::open(raw_file_path).await?;

        let mut offset = 0;
        while offset < lease.spec().size() {
            let n = std::cmp::min(4096, lease.spec().size() - offset);

            let mut buf = vec![];
            buf.resize(n as usize, 0);
            raw_file.read_exact(&mut buf).await?;

            let mut part = BlobData::default();
            part.set_data(buf);
            response.send(part).await?;

            offset += n;
        }

        drop(lease);

        Ok(())
    }

    /// Implementation of the Delete RPC.
    async fn delete_impl(&self, blob_id: &str) -> Result<()> {
        let lease = {
            let mut state = self.shared.state.lock().await;
            let blob = match state.blobs.get_mut(blob_id) {
                Some(v) => v,
                None => {
                    return Err(rpc::Status::not_found("No blob with the given id").into());
                }
            };

            if blob.ref_count != 0 {
                return Err(rpc::Status::failed_precondition("Can't delete an in-use blob").into());
            }

            // We should never have non-existent blobs in the map that aren't exclusively
            // locked as they are all deleted when the lease is dropped.
            assert!(blob.exists);

            blob.exclusive_lock = true;
            blob.ref_count = 1;
            blob.exists = false;

            BlobLease {
                shared: self.shared.clone(),
                spec: blob.spec.clone(),
            }
        };

        delete_blob_spec(self.shared.db.as_ref(), blob_id).await?;

        // TODO: In case we crash before this finishes, perform cleanup of non-existent
        // blobs on start up.
        fs::remove_dir_all(self.shared.dir.join(blob_id)).await?;

        drop(lease);

        Ok(())
    }

    // Getting a read path to a blob:
    // - If available locally, get it from there.
    // - Else, find replicas in meta db and download it.
}

/// Reference to a blob.
/// While this lease is active.
pub struct BlobLease {
    shared: Arc<Shared>,
    spec: BlobSpec,
}

impl Drop for BlobLease {
    fn drop(&mut self) {
        let shared = self.shared.clone();
        let blob_id = self.spec.id().to_string();
        task::spawn(async move {
            let mut state = shared.state.lock().await;
            let entry = state.blobs.get_mut(&blob_id).unwrap();
            entry.ref_count -= 1;
            entry.exclusive_lock = false;

            if !entry.exists {
                state.blobs.remove(&blob_id);
            }
        });
    }
}

impl BlobLease {
    pub fn spec(&self) -> &BlobSpec {
        &self.spec
    }

    pub fn raw_path(&self) -> PathBuf {
        self.shared.dir.join(self.spec.id()).join("raw")
    }

    pub fn extracted_dir(&self) -> PathBuf {
        self.shared.dir.join(self.spec.id()).join("extracted")
    }
}

pub struct BlobWriter {
    /// NOTE: We assume that we acquired an exclusive lock.
    lease: BlobLease,

    raw_file: File,

    hasher: SHA256Hasher,

    bytes_written: u64,
}

impl BlobWriter {
    pub async fn write(&mut self, data: &[u8]) -> Result<()> {
        self.bytes_written += data.len() as u64;
        if self.bytes_written > self.lease.spec().size() {
            // TODO: In this case, delete the partially written blob.
            println!("{} vs {}", self.bytes_written, self.lease.spec().size());
            return Err(rpc::Status::invalid_argument("Too many bytes written to the blob").into());
        }

        self.raw_file.write_all(data).await?;
        self.hasher.update(data);

        Ok(())
    }

    pub async fn finish(mut self) -> Result<()> {
        // TODO: On format failures, delete the partially written blob.

        if self.bytes_written != self.lease.spec().size() {
            return Err(
                rpc::Status::invalid_argument("Wrong number of bytes written to the blob").into(),
            );
        }

        // NOTE: We expect hex capitalization to also match in case our file system is
        // case sensitive.
        // TODO: Deduplicate the logic for hashing blobs.
        let hash = format!("sha256:{}", common::hex::encode(self.hasher.finish()));
        if hash != self.lease.spec().id() {
            return Err(rpc::Status::invalid_argument("Blob id did not match blob data").into());
        }

        self.raw_file.flush().await?;

        match self.lease.spec().format() {
            BlobFormat::UNKNOWN => {} // This should have been filtered out
            BlobFormat::TAR_ARCHIVE => {
                // TODO: Consider deferring extraction untul we actually need to use it.
                let extracted_dir = self.lease.extracted_dir();
                if !extracted_dir.exists().await {
                    common::async_std::fs::create_dir(&extracted_dir).await?;

                    let mut perms = extracted_dir.metadata().await?.permissions();
                    perms.set_mode(0o755);
                    common::async_std::fs::set_permissions(&extracted_dir, perms).await?;
                }

                let mut archive_reader =
                    compression::tar::Reader::open(self.lease.raw_path()).await?;
                archive_reader
                    .extract_files_with_modes(
                        extracted_dir.as_path().into(),
                        Some(0o644),
                        Some(0o755),
                    )
                    .await?;
            }
        }

        put_blob_spec(self.lease.shared.db.as_ref(), self.lease.spec().clone()).await?;

        {
            let mut state = self.lease.shared.state.lock().await;
            let mut entry = state.blobs.get_mut(self.lease.spec().id()).unwrap();
            entry.exists = true;
        }

        drop(self.lease);

        Ok(())
    }
}

#[async_trait]
impl BlobStoreService for BlobStore {
    async fn List(
        &self,
        request: rpc::ServerRequest<google::proto::empty::Empty>,
        response: &mut rpc::ServerResponse<BlobListResponse>,
    ) -> Result<()> {
        let state = self.shared.state.lock().await;
        for entry in state.blobs.values() {
            if !entry.exists {
                continue;
            }

            response.value.add_blob(entry.spec.clone());
        }

        Ok(())
    }

    async fn Upload(
        &self,
        mut request: rpc::ServerStreamRequest<BlobData>,
        response: &mut rpc::ServerResponse<google::proto::empty::Empty>,
    ) -> Result<()> {
        self.upload_impl(request, response).await
    }

    async fn Download(
        &self,
        request: rpc::ServerRequest<BlobDownloadRequest>,
        response: &mut rpc::ServerStreamResponse<BlobData>,
    ) -> Result<()> {
        self.download_impl(request, response).await
    }

    async fn Delete(
        &self,
        request: rpc::ServerRequest<BlobDeleteRequest>,
        response: &mut rpc::ServerResponse<google::proto::empty::Empty>,
    ) -> Result<()> {
        self.delete_impl(request.blob_id()).await
    }
}
