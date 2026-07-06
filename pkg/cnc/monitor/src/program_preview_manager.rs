use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base_error::*;
use cnc_monitor_proto::cnc::{MachineConfig, ProgramPreviewProto};
use common::io::Writeable;
use crypto::hasher::Hasher;
use crypto::sip::SipHasher;
use db_table::{query_one, ProtobufDB};
use executor::child_task::ChildTask;
use executor::sync::{AsyncMutex, AsyncVariable};
use executor::{lock, lock_async};
use protobuf::Message;

use crate::files::FileReference;
use crate::program::new_progress_tracker;
use crate::program_preview::ProgramPreview;
use crate::tables::ProgramPreviewTable;

pub struct ProgramPreviewManager {
    shared: Arc<Shared>,
}

struct Shared {
    db: Arc<ProtobufDB>,
    state: AsyncMutex<State>,
}

#[derive(Default)]
struct State {
    tasks: HashMap<PreviewKey, ProcessingTask>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct PreviewKey {
    file_id: u64,
    machine_config_hash: u64,
}

struct ProcessingTask {
    revision: u64,
    task: ChildTask,
    // A value of None implies that the task is done.
    progress: Arc<AsyncVariable<ProcessingState>>,
}

enum ProcessingState {
    Running(f32),
    Done,
    Error,
}

impl ProgramPreviewManager {
    pub fn new(db: Arc<ProtobufDB>) -> Self {
        Self {
            shared: Arc::new(Shared {
                db,
                state: AsyncMutex::default(),
            }),
        }
    }

    /// CANCEL SAFE
    ///
    /// TODO: Support cancelling execution if no machine references this
    /// config/file anymore.
    pub async fn get(
        &self,
        file: FileReference,
        machine_config: &MachineConfig,
        force_reprocess: bool,
    ) -> Result<ProgramPreviewReference> {
        let mut config = machine_config.clone();

        // TODO: Also hash the ProgramSummary so that we recompute things if that
        // changes.
        let config_key = {
            // Clear anything that shouldn't be relevant to the preview generation.
            config.clear_name();
            config.clear_model_name();
            config.clear_auto_connect();
            config.clear_clear_fields();
            config.clear_base_config();
            config.clear_device();
            for c in config.cameras_mut() {
                c.clear_device();
                c.clear_id();
            }

            let data = config.serialize()?;

            let mut h = SipHasher::default_rounds_with_key_halves(0, 0);
            h.update(&data[..]);
            h.finish_u64()
        };

        let shared = self.shared.clone();
        executor::spawn(async move {
            lock_async!(state <= shared.state.lock().await?, {
                Self::get_inner_impl(
                    &shared,
                    file,
                    config,
                    config_key,
                    force_reprocess,
                    &mut state,
                )
                .await
            })
        })
        .join()
        .await
    }

    async fn get_inner_impl(
        shared: &Arc<Shared>,
        file: FileReference,
        machine_config: MachineConfig,
        machine_config_hash: u64,
        force_reprocess: bool,
        state: &mut State,
    ) -> Result<ProgramPreviewReference> {
        let existing_preview = query_one!(
            shared.db,
            ProgramPreviewTable,
            "file_id = ? AND config_hash = ?",
            file.id(),
            machine_config_hash
        );

        let mut existing_in_progress = None;
        if let Some(mut existing_preview) = existing_preview {
            if force_reprocess {
                //
            } else if existing_preview.state().ready()
                || !existing_preview.state().error().is_empty()
            {
                // TODO: Dedup this.

                existing_preview.set_layer_images_url(format!(
                    "/data/files/{}/preview/{}/revision/{}/layers.bimg.zz",
                    base_radix::hex_encode(&file.id().to_be_bytes()),
                    base_radix::hex_encode(&machine_config_hash.to_be_bytes()),
                    base_radix::hex_encode(&existing_preview.revision().to_be_bytes())
                ));

                return Ok(ProgramPreviewReference {
                    proto: existing_preview,
                });
            } else {
                existing_in_progress = Some(existing_preview);
            }
        }

        let key = PreviewKey {
            file_id: file.id(),
            machine_config_hash,
        };

        if let Some(task) = state.tasks.get(&key) {
            let progress = task.progress.lock().await?.read_exclusive();

            let mut proto = ProgramPreviewProto::default();
            proto.set_file_id(file.id());
            proto.set_config_hash(machine_config_hash);
            proto.set_revision(task.revision);
            proto.state_mut().set_progress(match *progress {
                ProcessingState::Done | ProcessingState::Error => 1.0,
                ProcessingState::Running(v) => v,
            });
            return Ok(ProgramPreviewReference { proto });
        }

        if let Some(p) = existing_in_progress {
            let mut proto = ProgramPreviewProto::default();
            proto.set_file_id(file.id());
            proto.set_config_hash(machine_config_hash);
            proto.set_revision(p.revision());
            proto
                .state_mut()
                .set_error("Unknown failure occurred during processing.");
            shared.db.insert::<ProgramPreviewTable>(&proto).await?;
            return Ok(ProgramPreviewReference { proto });
        }

        let revision = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let mut proto = ProgramPreviewProto::default();
        proto.set_file_id(file.id());
        proto.set_config_hash(machine_config_hash);
        proto.set_revision(revision);
        proto.state_mut().set_progress(0.0);
        shared.db.insert::<ProgramPreviewTable>(&proto).await?;

        let progress = Arc::new(AsyncVariable::new(ProcessingState::Running(0.0)));

        let task = ChildTask::spawn(Self::compute_preview_task(
            Arc::downgrade(&shared),
            file,
            machine_config,
            machine_config_hash,
            revision,
            progress.clone(),
        ));
        state.tasks.insert(
            key,
            ProcessingTask {
                task,
                revision,
                progress: progress.clone(),
            },
        );

        return Ok(ProgramPreviewReference { proto });
    }

    async fn compute_preview_task(
        shared: Weak<Shared>,
        file: FileReference,
        machine_config: MachineConfig,
        machine_config_key: u64,
        revision: u64,
        task_progress: Arc<AsyncVariable<ProcessingState>>,
    ) {
        let task_key = PreviewKey {
            file_id: file.id(),
            machine_config_hash: machine_config_key,
        };

        let (progress_sender, mut progress_receiver) = new_progress_tracker();

        let res = ChildTask::spawn(async move {
            let preview = ProgramPreview::create(
                &file.path(),
                &machine_config,
                file.proto().program(),
                Some(progress_sender),
                false,
            )
            .await?;

            if !preview.layers_image.is_empty() {
                let summary_dir = file
                    .data_dir()
                    .join("preview")
                    .join(base_radix::hex_encode(&machine_config_key.to_be_bytes()))
                    .join("revision")
                    .join(base_radix::hex_encode(&revision.to_be_bytes()));
                file::create_dir_all(&summary_dir).await?;

                {
                    let mut f = file::LocalFile::open_with_options(
                        summary_dir.join("layers.bimg.zz"),
                        file::LocalFileOpenOptions::new().write(true).create(true),
                    )?;

                    for part in &preview.layers_image {
                        f.write_all(&part).await?;
                    }

                    f.flush().await?;
                }
            }

            Ok(preview)
        });

        while let Some(progress) = progress_receiver.wait().await {
            lock!(p <= task_progress.lock().await.unwrap(), {
                *p = ProcessingState::Running(progress);
                p.notify_all();
            });
        }

        let res: Result<ProgramPreview> = res.join().await;

        let mut preview = match res {
            Ok(v) => {
                let mut p = v.proto;
                p.state_mut().set_ready(true);
                p
            }
            Err(e) => {
                eprintln!("Failed to generate preview: {}", e);
                let mut proto = ProgramPreviewProto::default();
                proto.state_mut().set_error(e.to_string());
                proto
            }
        };

        preview.set_file_id(task_key.file_id);
        preview.set_config_hash(machine_config_key);
        preview.set_revision(revision);

        if let Some(shared) = shared.upgrade() {
            if let Err(e) = shared.db.insert::<ProgramPreviewTable>(&preview).await {
                eprintln!("Failed to save preview: {}", e);
                // Backoff to avoid repeated retries.
                executor::sleep(Duration::from_secs(10)).await;
            }
        }

        // Remove the task entry before marking as done.
        let task = {
            if let Some(shared) = shared.upgrade() {
                lock!(state <= shared.state.lock().await.unwrap(), {
                    state.tasks.remove(&task_key)
                })
            } else {
                None
            }
        };

        lock!(p <= task_progress.lock().await.unwrap(), {
            *p = ProcessingState::Done;
            p.notify_all();
        });

        // NOTE: This will cancel the currently running task.
        drop(task);
    }
}

pub struct ProgramPreviewReference {
    // shared: Arc<Shared>,
    proto: ProgramPreviewProto,
    // /// If present, then the preview is still being generated.
    // progress: Option<Arc<AsyncVariable<ProcessingState>>>,
}

impl ProgramPreviewReference {
    pub fn proto(&self) -> &ProgramPreviewProto {
        &self.proto
    }
}
