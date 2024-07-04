use std::{collections::HashMap, sync::Arc, time::SystemTime};

use base_error::*;
use cnc_monitor_proto::cnc::*;
use common::io::{Readable, Writeable};
use crypto::random::SharedRngExt;
use executor::lock;
use executor::sync::SyncMutex;
use file::LocalPathBuf;

use crate::db::ProtobufDB;
use crate::{
    change::{ChangeEvent, ChangePublisher},
    program::ProgramSummary,
    tables::FileTable,
};

/// Manages the state for all locally uploaded files.
pub struct FileManager {
    shared: Arc<Shared>,
}

struct Shared {
    state: SyncMutex<State>,
    files_dir: LocalPathBuf,
    db: Arc<ProtobufDB>,
    change_publisher: ChangePublisher,
}

#[derive(Default)]
struct State {
    files: HashMap<u64, FileEntry>,
}

struct FileEntry {
    proto: FileProto,
    ref_count: usize,
    exclusive_locked: bool,
}

impl FileEntry {
    fn new(proto: FileProto) -> Self {
        Self {
            proto,
            ref_count: 0,
            exclusive_locked: false,
        }
    }
}

impl FileManager {
    pub async fn create(
        files_dir: &LocalPathBuf,
        db: Arc<ProtobufDB>,
        change_publisher: ChangePublisher,
    ) -> Result<Self> {
        let mut state = State::default();
        for file in db.list::<FileTable>().await? {
            state.files.insert(file.id(), FileEntry::new(file));
        }

        let shared = Arc::new(Shared {
            state: SyncMutex::new(state),
            files_dir: files_dir.to_owned(),
            db,
            change_publisher,
        });

        // TODO: May need to re-schedule processing of any files that aren't processed.

        // TODO: Any files that aren't making progress of getting uploaded should get
        // cleaned up.

        Ok(Self { shared })
    }

    pub fn query_files(
        &self,
        query_id: Option<u64>,
        out: &mut QueryEntitiesResponse,
    ) -> Result<()> {
        self.shared.state.apply(|state| {
            for (file_id, file) in &state.files {
                if let Some(id) = query_id {
                    if id != *file_id {
                        continue;
                    }
                }

                out.add_files(Self::file_proto_with_urls(&self.shared, &file.proto));
            }
        })?;

        Ok(())
    }

    /// It is the callers responsibility to check the state.
    ///
    /// NOTE: The file is only guaranteed to not be deleted while the returned
    /// FileReference is not dropped.
    pub fn lookup(&self, file_id: u64) -> Result<FileReference> {
        // TODO: Also track which machines are using the file.
        self.acquire_file_lock(file_id, false)
    }

    /// CANCEL SAFE
    pub async fn start_file_upload(&self, name: &str, size: u64) -> Result<FileProto> {
        let id = crypto::random::global_rng().uniform::<u64>().await;

        let mut proto = FileProto::default();
        proto.set_id(id);
        proto.set_size(size);
        proto.set_name(name);
        proto.set_state(FileProto_State::UPLOADING);

        let shared = self.shared.clone();
        executor::spawn(async move {
            shared.db.insert::<FileTable>(&proto).await?;

            shared.state.apply(|state| {
                state.files.insert(id, FileEntry::new(proto.clone()));
            })?;

            shared
                .change_publisher
                .publish(ChangeEvent::new(EntityType::FILE, Some(id), false));

            Ok(proto)
        })
        .join()
        .await
    }

    // TODO: Make this cancel safe.
    pub async fn upload_file(
        &self,
        id: u64,
        size: u64,
        mut reader: Box<dyn Readable>,
    ) -> Result<()> {
        // Get an exclusive lock to the file.

        let mut file_lock = self.acquire_file_lock(id, true)?;

        if size != file_lock.proto.size() {
            return Err(rpc::Status::invalid_argument("Wrong size when uploading").into());
        }

        if file_lock.proto.state() != FileProto_State::UPLOADING {
            return Err(rpc::Status::failed_precondition(
                "This file isn't currently being uploaded",
            )
            .into());
        }

        file::create_dir_all(&file_lock.data_dir()).await?;

        let raw_path = file_lock.path();

        println!("Write to {}", raw_path.as_str());

        let mut file_writer = file::LocalFile::open_with_options(
            &raw_path,
            &file::LocalFileOpenOptions::new()
                .create_new(true)
                .write(true),
        )?;
        reader.pipe(&mut file_writer).await?;
        file_writer.flush().await?;

        let upload_time = SystemTime::now();

        file_lock.proto.set_upload_time(upload_time);
        file_lock.proto.set_state(FileProto_State::READY);

        // TODO: Need to limit max processing concurrency.
        let name = file_lock.proto.name().to_ascii_lowercase();
        if name.ends_with(".gcode") || name.ends_with(".nc") {
            let summary = ProgramSummary::create(&raw_path).await?;

            if let Some(thumb) = summary.best_thumbnail()? {
                // TODO: Switch this to use a unique path to avoid caching in case we ever
                // re-generate the thumbnails.
                file::write(file_lock.thumbnail_path(), thumb).await?;
                file_lock.proto.set_has_thumbnail(true);
            }

            file_lock.proto.set_program(summary.proto);
        }

        // TODO: Make a standard helper for this.
        let shared = self.shared.clone();
        executor::spawn::<_, Result<()>>(async move {
            shared.db.insert::<FileTable>(&file_lock.proto).await?;

            shared.state.apply(|state| {
                let entry = state.files.get_mut(&id).unwrap();
                entry.proto = file_lock.proto.clone();
            })?;

            shared
                .change_publisher
                .publish(ChangeEvent::new(EntityType::FILE, Some(id), false));

            drop(file_lock);

            Ok(())
        })
        .join()
        .await?;

        Ok(())
    }

    /// CANCEL SAFE
    pub async fn delete_file(&self, file_id: u64) -> Result<()> {
        // TODO: Need to support cancelling any uploading/processing that is running.

        let file_lock = self.acquire_file_lock(file_id, true)?;
        let shared = self.shared.clone();

        executor::spawn(async move {
            shared.db.remove::<FileTable>(&file_lock.proto).await?;

            shared.state.apply(|state| {
                state.files.remove(&file_id);
            })?;

            let dir = file_lock.data_dir();
            if file::exists(&file_lock.data_dir()).await? {
                file::remove_dir_all(&dir).await?;
            }

            shared.change_publisher.publish(ChangeEvent::new(
                EntityType::FILE,
                Some(file_id),
                false,
            ));

            Ok(())
        })
        .join()
        .await
    }

    pub async fn reprocess_file(&self, file_id: u64) -> Result<()> {
        let file_lock = self.acquire_file_lock(file_id, true)?;

        if file_lock.proto.state() != FileProto_State::READY {
            return Err(rpc::Status::failed_precondition(
                "Only files in the READY state can be reprocessed",
            )
            .into());
        }

        todo!()
    }

    fn acquire_file_lock(&self, file_id: u64, exclusive: bool) -> Result<FileReference> {
        self.shared.state.apply(|state| {
            let entry = match state.files.get_mut(&file_id) {
                Some(v) => v,
                None => {
                    return Err(rpc::Status::not_found("No file with the given id found").into())
                }
            };

            if entry.ref_count > 0 && (entry.exclusive_locked || exclusive) {
                return Err(rpc::Status::aborted("Conflicting exclusive lock(s) on file.").into());
            }

            entry.ref_count += 1;
            entry.exclusive_locked = exclusive;

            Ok(FileReference {
                file_id,
                proto: entry.proto.clone(),
                shared: self.shared.clone(),
            })
        })?
    }

    fn file_data_dir(shared: &Shared, file_id: u64) -> LocalPathBuf {
        shared
            .files_dir
            .join(base_radix::hex_encode(&file_id.to_be_bytes()))
    }

    fn file_raw_path(shared: &Shared, file_id: u64) -> LocalPathBuf {
        Self::file_data_dir(shared, file_id).join("raw")
    }

    fn file_thumbnail_path(shared: &Shared, file_id: u64) -> LocalPathBuf {
        Self::file_data_dir(shared, file_id).join("thumbnail")
    }

    fn file_proto_with_urls(shared: &Shared, proto: &FileProto) -> FileProto {
        let mut proto = proto.clone();

        let base = format!(
            "/data/files/{}",
            base_radix::hex_encode(&proto.id().to_be_bytes())
        );

        proto.urls_mut().set_raw_url(format!("{}/raw", base));

        if proto.has_thumbnail() {
            proto
                .urls_mut()
                .set_thumbnail_url(format!("{}/thumbnail", base));
        }

        proto
    }
}

pub struct FileReference {
    file_id: u64,
    proto: FileProto,
    shared: Arc<Shared>,
}

impl Clone for FileReference {
    fn clone(&self) -> Self {
        self.shared.state.apply(|state| {
            state.files.get_mut(&self.file_id).map(|entry| {
                entry.ref_count += 1;
            });
        });

        Self {
            file_id: self.file_id,
            proto: self.proto.clone(),
            shared: self.shared.clone(),
        }
    }
}

impl Drop for FileReference {
    fn drop(&mut self) {
        let _ = self.shared.state.apply(|state| {
            state.files.get_mut(&self.file_id).map(|entry| {
                entry.ref_count -= 1;
            });
        });
    }
}

impl FileReference {
    pub fn id(&self) -> u64 {
        self.file_id
    }

    pub fn proto(&self) -> &FileProto {
        &self.proto
    }

    pub fn proto_with_urls(&self) -> FileProto {
        FileManager::file_proto_with_urls(&self.shared, &self.proto)
    }

    fn data_dir(&self) -> LocalPathBuf {
        self.shared
            .files_dir
            .join(base_radix::hex_encode(&self.file_id.to_be_bytes()))
    }

    /// Path to the raw data file for this file.
    pub fn path(&self) -> LocalPathBuf {
        FileManager::file_raw_path(&self.shared, self.file_id)
    }

    pub fn thumbnail_path(&self) -> LocalPathBuf {
        FileManager::file_thumbnail_path(&self.shared, self.file_id)
    }
}
