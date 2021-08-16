mod backoff;
pub mod main;

use std::collections::HashMap;
use std::fs::Permissions;
use std::os::unix::prelude::PermissionsExt;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use common::async_std::channel;
use common::async_std::io::prelude::WriteExt;
use common::async_std::sync::Mutex;
use common::errors::*;
use common::task::ChildTask;
use crypto::hasher::Hasher;
use nix::unistd::chown;
use nix::unistd::Gid;
use protobuf::text::parse_text_proto;

use crate::proto::config::*;
use crate::proto::log::*;
use crate::proto::service::*;
use crate::proto::task::*;
use crate::runtime::ContainerRuntime;

/// Directory for storing data for uploaded blobs.
///
/// In this directory, the following files will be stored:
/// - './{BLOB_ID}/raw' : Raw version of the blob as uploaded.
/// - './{BLOB_ID}/extracted/' : Directory which contains all of the files
/// - './{BLOB_ID}/metadata' : Present once the blob has been fully ingested.
///   Currently this file is always empty.
const BLOB_DATA_DIR: &'static str = "/opt/dacha/container/blob";

const PERSISTENT_DIR: &'static str = "/opt/dacha/container/persistent";

const GRACEFUL_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

struct Task {
    /// Spec that was used to start this task.
    spec: TaskSpec,

    /// Id of the most recent container running this task.
    container_id: Option<String>,

    state: TaskState,

    /// The task was recently created or updated so we are waiting for the task
    /// to be started using the latest TaskSpec.
    ///
    /// Will be reset to false once we have entired the Starting|Running state.
    pending_update: bool,
}

enum TaskState {
    /// We are still launching on resources to become available or still
    /// creating the environment that will run this task.
    Pending,

    /// In this state, we have a running container for this task.
    Running,

    /// In this state, we already sent a SIGINT to the task and are waiting for
    /// it to stop on its own.
    Stopping {
        timer_id: usize,
        timeout_task: ChildTask,
    },

    /// We were just in the Stopping state and sent a SIGKILL to the container
    /// because it was taking too long to stop.
    /// We are currently waiting for the container runtime to report that the
    /// container is completely dead.
    ForceStopping,

    /// The task's container is dead and we are waiting a reasonable amount of
    /// time before retrying.
    RestartBackoff,

    /// The container has exited and there is no plan to restart it.
    Terminal,
}

/*
State of a container:
    Creating|Running or Stopped

State of a task:

- Running
    - We've created a container for it is

- Stopping(start_time)
    - When entered, we will send a SIGINT to the task
    - If still in this state for N seconds, we will send a SIGKILL
    - Just wait for the container to enter a Stopped state.

- Killed

- Restarting
    - We must wait until the container has been killed
    - Once it has killed,

Creating a task:


TODO: Verify that sending a kill to the runtime doesn't cause an error if the container just recently died and we didn't process the event notification yet.

- Deleting
*/

enum NodeEvent {
    ContainerStateChange { container_id: String },

    StopTimeout { task_name: String, timer_id: usize },

    RuntimeEnded(Result<()>),
}

#[derive(Clone)]
pub struct Node {
    shared: Arc<NodeShared>,
}

struct NodeShared {
    runtime: Arc<ContainerRuntime>,
    event_channel: (channel::Sender<NodeEvent>, channel::Receiver<NodeEvent>),
    state: Mutex<NodeState>,

    last_timer_id: AtomicUsize,
}

struct NodeState {
    tasks: Vec<Task>,
    inner: NodeStateInner,
}

struct NodeStateInner {
    container_id_to_task_name: HashMap<String, String>,
}

impl Node {
    pub async fn create() -> Result<Self> {
        let runtime = ContainerRuntime::create().await?;
        Ok(Self {
            shared: Arc::new(NodeShared {
                runtime,
                state: Mutex::new(NodeState {
                    tasks: vec![],
                    inner: NodeStateInner {
                        container_id_to_task_name: HashMap::new(),
                    },
                }),
                event_channel: channel::unbounded(),
                last_timer_id: AtomicUsize::new(0),
            }),
        })
    }

    pub fn run(&self) -> impl std::future::Future<Output = Result<()>> {
        self.clone().run_event_loop()
    }

    async fn run_event_loop(self) -> Result<()> {
        let runtime_task = {
            let runtime = self.shared.runtime.clone();
            let sender = self.shared.event_channel.0.clone();

            ChildTask::spawn(async move {
                let _ = sender
                    .send(NodeEvent::RuntimeEnded(runtime.run().await))
                    .await;
            })
        };

        let runtime_listener_task = {
            let receiver = self.shared.runtime.add_event_listener().await;
            let sender = self.shared.event_channel.0.clone();

            ChildTask::spawn(async move {
                while let Ok(container_id) = receiver.recv().await {
                    let _ = sender
                        .send(NodeEvent::ContainerStateChange { container_id })
                        .await;
                }
            })
        };

        loop {
            let event = self.shared.event_channel.1.recv().await?;
            match event {
                NodeEvent::ContainerStateChange { container_id } => {
                    // Currently this always occurs when the container had been killed

                    let mut state_guard = self.shared.state.lock().await;
                    let state = &mut *state_guard;

                    let task_name = match state.inner.container_id_to_task_name.get(&container_id) {
                        Some(v) => v.clone(),
                        None => {
                            eprintln!(
                                "Container id is not associated with a task: {}",
                                container_id
                            );
                            continue;
                        }
                    };

                    let task = state
                        .tasks
                        .iter_mut()
                        .find(|t| t.spec.name() == task_name)
                        .unwrap();

                    if task.pending_update {
                        // TODO: This needs to put the task into an error task if this fails.
                        if let Err(e) = self
                            .transition_task_to_running(&mut state.inner, task)
                            .await
                        {
                            // Report this error back to the client.
                            eprintln!("Failed to start running task: {}", e);

                            // TODO: We should determine if it is a retryable error (in which case
                            // we can retry based on the restart policy)
                            task.state = TaskState::Terminal;
                        }
                    } else {
                        // TODO: Need to check the restart policy to see if we should restart the
                        // container.

                        task.state = TaskState::Terminal;
                    }
                }
                NodeEvent::StopTimeout {
                    task_name,
                    timer_id: event_timer_id,
                } => {
                    // If the timer id matches the one in the current Stopped state, then we'll send
                    // a SIGKILL

                    let mut state = self.shared.state.lock().await;

                    let task = match state.tasks.iter_mut().find(|t| t.spec.name() == task_name) {
                        Some(t) => t,
                        None => {
                            // Most likely a race condition with the timer event being processed
                            // after the task was deleted.
                            continue;
                        }
                    };

                    let mut should_force_stop = false;
                    if let TaskState::Stopping { timer_id, .. } = &task.state {
                        if *timer_id == event_timer_id {
                            should_force_stop = true;
                        }
                    }

                    if should_force_stop {
                        let container_id = task.container_id.as_ref().unwrap();
                        self.shared
                            .runtime
                            .kill_container(container_id, nix::sys::signal::Signal::SIGKILL)
                            .await?;

                        task.state = TaskState::ForceStopping;
                    }
                }
                NodeEvent::RuntimeEnded(result) => {
                    if result.is_ok() {
                        return Err(err_msg("Container runtime ended early"));
                    }

                    return result;
                }
            }
        }
    }

    async fn transition_task_to_running(
        &self,
        state_inner: &mut NodeStateInner,
        task: &mut Task,
    ) -> Result<()> {
        // TODO: Parse this at startup time.
        let mut container_config = ContainerConfig::default();

        // TODO: Change the user id of the devpts to the one.
        parse_text_proto(
            r#"
            process {
                user {
                    uid: 100001
                    gid: 100001
                    additional_gids: [100002]
                }
            }
            mounts: [
                {
                    destination: "/proc"
                    type: "proc"
                    source: "proc"
                    options: ["noexec", "nosuid", "nodev"]
                },
                {
                    destination: "/usr/bin"
                    source: "/usr/bin"
                    options: ["bind", "ro"]
                },
                {
                    destination: "/lib64"
                    source: "/lib64"
                    options: ["bind", "ro"]
                },
                {
                    destination: "/usr/lib"
                    source: "/usr/lib"
                    options: ["bind", "ro"]
                },
                {
                    destination: "/dev/pts"
                    type: "devpts"
                    source: "devpts"
                    options: [
                        "nosuid",
                        "noexec",
                        "newinstance",
                        "ptmxmode=0666",
                        "gid=100001"
                    ]
                },
                {
                    destination: "/dev/null",
                    source: "/dev/null",
                    options: ["bind"]
                },
                {
                    destination: "/dev/zero",
                    source: "/dev/zero",
                    options: ["bind"]
                },
                {
                    destination: "/dev/random",
                    source: "/dev/random",
                    options: ["bind"]
                },
                {
                    destination: "/dev/urandom",
                    source: "/dev/urandom",
                    options: ["bind"]
                }
            ]
        "#,
            &mut container_config,
        )?;

        // container_config.process_mut().set_terminal(true);

        for arg in task.spec.args() {
            container_config.process_mut().add_args(arg.clone());
        }
        for val in task.spec.env() {
            container_config.process_mut().add_env(val.clone());
        }

        for port in task.spec.ports() {
            if port.number() == 0 {
                return Err(rpc::Status::invalid_argument(format!(
                    "Port not assigned a number: {}",
                    port.name()
                ))
                .into());
            }

            let env_name = port.name().to_uppercase().replace("-", "_");

            container_config
                .process_mut()
                .add_env(format!("PORT_{}={}", env_name, port.number()));
        }

        for volume in task.spec.volumes() {
            let mut mount = ContainerMount::default();
            mount.set_destination(format!("/volumes/{}", volume.name()));

            match volume.source_case() {
                TaskSpec_VolumeSourceCase::BlobId(blob_id) => {
                    let blob_dir = common::async_std::path::Path::new(BLOB_DATA_DIR).join(blob_id);

                    let metadata_path = blob_dir.join("metadata");
                    if !metadata_path.exists().await {
                        return Err(rpc::Status::not_found(format!(
                            "Blob for volume {} doesn't exist",
                            volume.name()
                        ))
                        .into());
                    }

                    mount.set_source(blob_dir.join("extracted").to_str().unwrap().to_string());

                    mount.add_options("bind".into());
                    mount.add_options("ro".into());
                }
                TaskSpec_VolumeSourceCase::PersistentName(name) => {
                    let dir = common::async_std::path::Path::new(PERSISTENT_DIR).join(name);
                    let dir_str = dir.to_str().unwrap().to_string();

                    if !dir.exists().await {
                        common::async_std::fs::create_dir_all(&dir).await?;
                        chown(dir_str.as_str(), None, Some(Gid::from_raw(100002)))?;

                        let mut perms = dir.metadata().await?.permissions();
                        perms.set_mode(0o770 | libc::S_ISGID);
                        common::async_std::fs::set_permissions(&dir, perms).await?;
                    }

                    mount.add_options("bind".into());
                    mount.set_source(dir_str);
                }
                TaskSpec_VolumeSourceCase::Unknown => {
                    return Err(
                        rpc::Status::invalid_argument("No source configured for volume").into(),
                    );
                }
            }

            container_config.add_mounts(mount);
        }

        // TODO: Move this after the start_container so that the operation is atomic?
        if task.pending_update {
            // TODO: Reset any backoff state.
            task.pending_update = false;
        }

        let container_id = self
            .shared
            .runtime
            .start_container(&container_config)
            .await?;

        if let Some(old_container_id) = task.container_id.take() {
            state_inner
                .container_id_to_task_name
                .remove(&old_container_id);
        }
        state_inner
            .container_id_to_task_name
            .insert(container_id.clone(), task.spec.name().to_string());

        task.container_id = Some(container_id.clone());
        task.state = TaskState::Running;

        Ok(())
    }

    async fn transition_task_to_stopping(&self, task: &mut Task) -> Result<()> {
        let container_id = task.container_id.as_ref().unwrap();
        self.shared
            .runtime
            .kill_container(container_id, nix::sys::signal::Signal::SIGINT)
            .await?;

        // TODO: Instead use the task id of the child task.
        let timer_id = self
            .shared
            .last_timer_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let task_name = task.spec.name().to_string();
        let timeout_sender = self.shared.event_channel.0.clone();
        let timeout_task = ChildTask::spawn(async move {
            common::async_std::task::sleep(GRACEFUL_SHUTDOWN_TIMEOUT.clone()).await;
            let _ = timeout_sender
                .send(NodeEvent::StopTimeout {
                    task_name,
                    timer_id,
                })
                .await;
        });

        task.state = TaskState::Stopping {
            timer_id,
            timeout_task,
        };

        Ok(())
    }
}

#[async_trait]
impl ContainerNodeService for Node {
    async fn Query(
        &self,
        request: rpc::ServerRequest<QueryRequest>,
        response: &mut rpc::ServerResponse<QueryResponse>,
    ) -> Result<()> {
        let containers = self.shared.runtime.list_containers().await;
        for container in containers {
            response.add_container(container);
        }

        Ok(())
    }

    // async fn Start(&self, request: rpc::ServerRequest<StartRequest>,
    //                 response: &mut rpc::ServerResponse<StartResponse>) ->
    // Result<()> {     let config = request.value.config();
    //     let id = self.shared.runtime.start_container(config).await?;
    //     response.value.set_container_id(id);
    //     Ok(())
    // }

    async fn StartTask(
        &self,
        request: rpc::ServerRequest<StartTaskRequest>,
        response: &mut rpc::ServerResponse<StartTaskResponse>,
    ) -> Result<()> {
        let mut state_guard = self.shared.state.lock().await;
        let state = &mut *state_guard;

        let existing_task = state
            .tasks
            .iter_mut()
            .find(|t| t.spec.name() == request.task_spec().name());

        let task = {
            if let Some(task) = existing_task {
                // TODO: Consider preserving the previous task_spec until the new one is added.
                task.spec = request.task_spec().clone();
                task
            } else {
                state.tasks.push(Task {
                    spec: request.task_spec().clone(),
                    container_id: None,
                    state: TaskState::Pending,
                    pending_update: false,
                });
                state.tasks.last_mut().unwrap()
            }
        };

        task.pending_update = true;

        match &task.state {
            TaskState::Pending | TaskState::RestartBackoff | TaskState::Terminal => {
                self.transition_task_to_running(&mut state.inner, task)
                    .await?;
            }
            TaskState::Running => {
                self.transition_task_to_stopping(task).await?;
            }
            TaskState::Stopping { .. } | TaskState::ForceStopping => {
                // We don't need to anything. Once the container finishes
                // stopping, the new container will be brought
                // up.
            }
        }

        Ok(())
    }

    // TODO: When the Node closes, we should kill all tasks that it has

    async fn GetLogs(
        &self,
        request: rpc::ServerRequest<LogRequest>,
        response: &mut rpc::ServerStreamResponse<LogEntry>,
    ) -> Result<()> {
        let container_id = {
            let state = self.shared.state.lock().await;
            let task = state
                .tasks
                .iter()
                .find(|t| t.spec.name() == request.task_name())
                .ok_or_else(|| {
                    Error::from(rpc::Status {
                        code: rpc::StatusCode::NotFound,
                        message: format!("No task found with name: {}", request.task_name()),
                    })
                })?;

            task.container_id.clone().unwrap()
        };

        // TODO: If the container is being shutdown then we may temporarily get the
        // wrong container id
        println!("GetLogs Container Id: {}", container_id);

        // TODO: Support log seeking.

        let mut log_reader = self.shared.runtime.open_log(&container_id).await?;

        println!("Log opened!");

        // TODO: This loop seems to keep running even if I close the request
        let mut num_ended = 0;
        loop {
            let entry = log_reader.read().await?;
            if let Some(entry) = entry {
                let end_stream = entry.end_stream();

                response.send(entry).await?;

                // TODO: Check that we got an end_stream on all the streams.
                if end_stream {
                    num_ended += 1;
                    if num_ended == 2 {
                        break;
                    }
                }
            } else {
                // TODO: Replace with receiving a notification.
                common::async_std::task::sleep(std::time::Duration::from_millis(100)).await;
            }
        }

        println!("Done logs!");

        Ok(())
    }

    async fn WriteInput(
        &self,
        mut request: rpc::ServerStreamRequest<WriteInputRequest>,
        _response: &mut rpc::ServerResponse<EmptyMessage>,
    ) -> Result<()> {
        loop {
            let input = match request.recv().await? {
                Some(v) => v,
                None => break,
            };

            // TODO: If we require that all messages be for the same task_name and process
            // id, then we can cache this value instead of looking it up every
            // time.
            let container_id = {
                let state = self.shared.state.lock().await;
                let task = state
                    .tasks
                    .iter()
                    .find(|t| t.spec.name() == input.task_name())
                    .ok_or_else(|| {
                        Error::from(rpc::Status {
                            code: rpc::StatusCode::NotFound,
                            message: format!("No task found with name: {}", input.task_name()),
                        })
                    })?;

                task.container_id.clone().unwrap()
            };

            self.shared
                .runtime
                .write_to_stdin(&container_id, input.data())
                .await?;
        }

        Ok(())
    }

    async fn UploadBlob(
        &self,
        mut request: rpc::ServerStreamRequest<BlobData>,
        response: &mut rpc::ServerResponse<EmptyMessage>,
    ) -> Result<()> {
        let first_part = request.recv().await?.ok_or_else(|| {
            rpc::Status::invalid_argument("Expected at least one request message")
        })?;

        // TODO: Obtain an exclusive lock via the FS on the OS on /opt/container to
        // ensure that this is consitent. TODO: Filter '..', absolute paths,
        // etc.
        let blob_dir = common::async_std::path::Path::new(BLOB_DATA_DIR).join(first_part.id());

        let metadata_path = blob_dir.join("metadata");
        if metadata_path.exists().await {
            return Err(rpc::Status {
                code: rpc::StatusCode::AlreadyExists,
                message: "Blob already exists".into(),
            }
            .into());
        }

        // Create the blob dir.
        // If the directory already exists, then likely a previous attempt to upload
        // failed, so we'll just retry.
        if !blob_dir.exists().await {
            common::async_std::fs::create_dir_all(&blob_dir).await?;
        }

        let mut raw_file_path = blob_dir.join("raw");

        let mut raw_file = common::async_std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&raw_file_path)
            .await?;

        let mut hasher = crypto::sha256::SHA256Hasher::default();

        raw_file.write_all(first_part.data()).await?;
        hasher.update(first_part.data());

        while let Some(part) = request.recv().await? {
            raw_file.write_all(part.data()).await?;
            hasher.update(part.data());
        }

        // NOTE: We expect hex capitalization to also match in case our file system is
        // case sensitive.
        let hash = common::hex::encode(hasher.finish());
        if hash != first_part.id() {
            return Err(rpc::Status::invalid_argument("Blob id did not match blob data").into());
        }

        raw_file.flush().await?;

        let extracted_dir = blob_dir.join("extracted");
        if !extracted_dir.exists().await {
            common::async_std::fs::create_dir(&extracted_dir).await?;

            let mut perms = blob_dir.metadata().await?.permissions();
            perms.set_mode(0o755);
            common::async_std::fs::set_permissions(&extracted_dir, perms).await?;
        }

        let mut archive_reader = compression::tar::Reader::open(&raw_file_path).await?;
        archive_reader
            .extract_files_with_modes(extracted_dir.as_path().into(), Some(0o644), Some(0o755))
            .await?;

        // Create an empty metadata sentinel file.
        common::async_std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&metadata_path)
            .await?;

        Ok(())
    }
}
