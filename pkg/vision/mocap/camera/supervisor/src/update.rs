use std::sync::Arc;
use std::time::Duration;
use std::os::unix::process::CommandExt;

use common::errors::*;
use executor::sync::AsyncMutex;
use mocap_proto::mocap::*;
use file::{LocalPath, LocalPathBuf};
use executor::channel;


const PARTITION_MOUNTS: &'static [&'static str] = &[
    "/",
    "/boot/firmware"
];


#[derive(Default)]
pub struct Updater {
    shared: Arc<Shared>
}

#[derive(Default)]
struct Shared {
    update_lock: AsyncMutex<()>,
}

enum PendingOperation {
    InstallDeb {
        input_path: LocalPathBuf
    },
    WriteFile {
        input_path: LocalPathBuf,
        output_path: LocalPathBuf,
    }
}

impl Updater {
    // NOTE: If this fails, then likely there is a file still
    // open as writeable by some program.
    //
    // e.g. 'sudo mount -o remount,ro /'
    pub fn toggle_writeable_fs(writeable: bool) -> Result<()> {
        for path in PARTITION_MOUNTS {
            let status = std::process::Command::new("mount")
                .args(&["-o", if writeable { "remount,rw" } else { "remount,ro" }, path])
                .status()?;
            if !status.success() {
                return Err(err_msg("Failed to remount partition"));
            }
        }

        Ok(())
    }

    pub async fn update(
        &self,
        mut req_stream: rpc::ServerStreamRequest<UpdateRequest>,
        mut res_stream: &mut rpc::ServerStreamResponse<'_, UpdateResponse>
    ) -> Result<()> {

        let (res_sender, res_receiver) = channel::unbounded();

        // Run in a separate task so that it can't be cancelled unsafely.
        let thread = executor::spawn(Self::update_inner(self.shared.clone(), req_stream, res_sender));

        while let Ok(v) = res_receiver.recv().await {
            res_stream.send(v).await?;
        }

        thread.join().await
    }

    async fn update_inner(
        shared: Arc<Shared>,
        mut req_stream: rpc::ServerStreamRequest<UpdateRequest>,
        mut res_sender: channel::Sender<UpdateResponse>
    ) -> Result<()> {

        let update_lock = match shared.update_lock.try_lock()? {
            Some(v) => v,
            None => {
                return Err(rpc::Status::failed_precondition(
                    "Update already in progress.",
                )
                .into())
            }
        };

        {
            let req = req_stream.recv().await?
                .ok_or_else(|| Error::from(rpc::Status::invalid_argument("No first request")))?;

            if !req.has_start_update() {
                return Err(rpc::Status::invalid_argument("Expected first request to be a start_update").into());
            }
        }

        // TODO: Always disable this if this future is dropped.
        Self::toggle_writeable_fs(true)?;

        let data_path = LocalPath::new("/opt/mocap/supervisor/update");

        // Clearing any old update.
        if file::exists(&data_path).await? {
            file::remove_dir_all(&data_path).await?;
        }

        file::create_dir_all(data_path).await?;

        res_sender.send(UpdateResponse::default()).await?;

        let mut pending_ops = vec![];

        let mut fragment_index = 0;

        while let Some(req) = req_stream.recv().await? {
            let fragment_path = data_path.join(format!("{:08}", fragment_index));

            match req.command_case() {
                UpdateRequestCommandCase::StartUpdate(cmd) => {
                    return Err(rpc::Status::invalid_argument("Already received a start_update").into());
                }
                UpdateRequestCommandCase::PayloadChunk(c) => {
                    file::append(&fragment_path, req.payload_chunk()).await?;
                    res_sender.send(UpdateResponse::default()).await?;
                }
                UpdateRequestCommandCase::InstallDeb(c) => {
                    pending_ops.push(PendingOperation::InstallDeb {
                        input_path: fragment_path.clone(),
                    });
                    fragment_index += 1;

                    res_sender.send(UpdateResponse::default()).await?;
                }
                UpdateRequestCommandCase::InstallImage(c) => {
                    // TODO:
                    return Err(rpc::Status::invalid_argument("Unsupported command type").into());
                }
                UpdateRequestCommandCase::WriteFile(c) => {
                    pending_ops.push(PendingOperation::WriteFile {
                        input_path: fragment_path.clone(),
                        output_path: LocalPath::new(c.path()).to_owned()
                    });
                    fragment_index += 1;


                    res_sender.send(UpdateResponse::default()).await?;
                }
                UpdateRequestCommandCase::CommitUpdate(c) => {
                    break;
                }
                UpdateRequestCommandCase::NOT_SET => {
                    return Err(rpc::Status::invalid_argument("Unsupported command type").into());
                }
            }
        }

        for op in pending_ops {
            match op {
                PendingOperation::InstallDeb { input_path } => {
                    let mut child = std::process::Command::new("dpkg")
                        .arg("-i").arg(&input_path)
                        // Fully isolated so we can update the supervisor itself.
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::inherit())
                        .stderr(std::process::Stdio::inherit())
                        .process_group(0) 
                        .spawn()?;
                    
                    let status = child.wait()
                        .map_err(|_| Error::from(rpc::Status::internal("Failed while waiting for dpkg")))?;

                    if !status.success() {
                        return Err(rpc::Status::unknown("dpkg -i failed").into());
                    }

                }
                PendingOperation::WriteFile { input_path, output_path } => {
                    // TODO: This will fail if we try writing an empty file.

                    // If the file is in use, then it needs to be unlinked first to allow overwriting it.
                    if file::exists(&output_path).await? {
                        file::remove_file(&output_path).await?;
                    }

                    file::copy(&input_path, &output_path).await?;
                }
            }
            
        }

        let mut res = UpdateResponse::default();
        res.set_commited(true);
        res_sender.send(res).await?;

        // Wait for any background cleanup of writeable file descriptors to finish. 
        executor::sleep(Duration::from_secs(1)).await?;
        
        if let Err(e) = Self::toggle_writeable_fs(false) {
            eprintln!("Failed to mark FS as read only: {}", e);
        }

        drop(update_lock);

        Ok(())
    }

}