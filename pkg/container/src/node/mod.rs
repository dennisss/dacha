mod backoff;
pub mod main;
pub mod shadow;
mod tasks_table;

use std::collections::HashMap;
use std::fs::Permissions;
use std::os::unix::prelude::{MetadataExt, PermissionsExt};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Duration;

use common::async_std::channel;
use common::async_std::io::prelude::WriteExt;
use common::async_std::path::{Path, PathBuf};
use common::async_std::sync::Mutex;
use common::errors::*;
use common::task::ChildTask;
use crypto::hasher::Hasher;
use nix::unistd::chown;
use nix::unistd::Gid;
use protobuf::text::parse_text_proto;
use sstable::{EmbeddedDB, EmbeddedDBOptions};

use crate::node::shadow::*;
use crate::proto::config::*;
use crate::proto::log::*;
use crate::proto::node::*;
use crate::proto::service::*;
use crate::proto::task::*;
use crate::runtime::ContainerRuntime;

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
pub struct NodeContext {
    /// All groups present on the system.
    pub system_groups: HashMap<String, u32>,

    /// All user ids which the node is allowed to impersonate.
    pub sub_uids: Vec<IdRange>,

    /// All group ids which the node is allowed to impersonate.
    pub sub_gids: Vec<IdRange>,

    /// User id range from which we will pull new user ids to run task
    /// containers.
    pub container_uids: IdRange,

    /// Similar to container_uids, but for group ids. This is also used for
    /// allocated file system ACL groups.
    pub container_gids: IdRange,
}

#[derive(Clone)]
pub struct Node {
    shared: Arc<NodeShared>,
}

struct NodeShared {
    context: NodeContext,
    config: NodeConfig,

    db: EmbeddedDB,

    runtime: Arc<ContainerRuntime>,
    event_channel: (channel::Sender<NodeEvent>, channel::Receiver<NodeEvent>),
    state: Mutex<NodeState>,

    usb_context: Arc<usb::Context>,

    last_timer_id: AtomicUsize,
}

struct NodeState {
    tasks: Vec<Task>,
    inner: NodeStateInner,
}

struct NodeStateInner {
    container_id_to_task_name: HashMap<String, String>,

    next_uid: u32,
    next_gid: u32,

    /// Map of host paths to the number of running tasks referencing them. This
    /// is used to implement exclusive locks to volumes/devices.
    mounted_paths: HashMap<String, usize>,
}

impl Node {
    pub async fn create(context: &NodeContext, config: &NodeConfig) -> Result<Self> {
        let mut db_options = EmbeddedDBOptions::default();
        db_options.create_if_missing = true;

        let db = EmbeddedDB::open(Path::new(config.data_dir()).join("db"), db_options).await?;

        let inner = NodeStateInner {
            container_id_to_task_name: HashMap::new(),
            // TODO: Preserve these across reboots and attempt to not re-use ids already
            // referenced in the file system.
            next_uid: context.container_uids.start_id,
            next_gid: context.container_gids.start_id,
            mounted_paths: HashMap::new(),
        };

        let usb_context = usb::Context::create()?;

        let runtime = ContainerRuntime::create(Path::new(config.data_dir()).join("run")).await?;
        let inst = Self {
            shared: Arc::new(NodeShared {
                context: context.clone(),
                config: config.clone(),
                db,
                runtime,
                state: Mutex::new(NodeState {
                    tasks: vec![],
                    inner,
                }),
                event_channel: channel::unbounded(),
                last_timer_id: AtomicUsize::new(0),
                usb_context,
            }),
        };

        let all_tasks = tasks_table::list_tasks(&inst.shared.db).await?;
        for task_spec in all_tasks {
            if !task_spec.persistent() {
                continue;
            }

            // TODO: Don't allow the failure of this to block node startup.
            inst.start_task(&task_spec).await?;
        }

        Ok(inst)
    }

    // TODO: Implement graceful node shutdown which terminates all the inner nodes.

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

        drop(runtime_task);
        drop(runtime_listener_task);
    }

    /// Directory for storing data for uploaded blobs.
    ///
    /// In this directory, the following files will be stored:
    /// - './{BLOB_ID}/raw' : Raw version of the blob as uploaded.
    /// - './{BLOB_ID}/extracted/' : Directory which contains all of the files
    /// - './{BLOB_ID}/metadata' : Present once the blob has been fully
    ///   ingested. Currently this file is always empty.
    fn blob_data_dir(&self) -> PathBuf {
        Path::new(self.shared.config.data_dir()).join("blob")
    }

    fn persistent_data_dir(&self) -> PathBuf {
        Path::new(self.shared.config.data_dir()).join("persistent")
    }

    async fn transition_task_to_running(
        &self,
        state_inner: &mut NodeStateInner,
        task: &mut Task,
    ) -> Result<()> {
        // TODO: Parse this at startup time.
        let mut container_config = self.shared.config.container_template().clone();

        // TODO: Check for overflows of the count in the range.
        let process_uid = state_inner.next_uid;
        state_inner.next_uid += 1;

        let process_gid = state_inner.next_gid;
        state_inner.next_gid += 1;

        container_config
            .process_mut()
            .user_mut()
            .set_uid(process_uid);
        container_config
            .process_mut()
            .user_mut()
            .set_gid(process_gid);

        for additional_group in task.spec.additional_groups() {
            let gid = *self
                .shared
                .context
                .system_groups
                .get(additional_group)
                .ok_or_else(|| {
                    rpc::Status::invalid_argument(format!(
                        "No group found named: {}",
                        additional_group
                    ))
                })?;

            let mut matched = false;
            for range in &self.shared.context.sub_gids {
                if range.contains(gid) {
                    matched = true;
                    break;
                }
            }

            if !matched {
                return Err(rpc::Status::invalid_argument(format!(
                    "Node is not allowed to delegate group: {}",
                    additional_group
                ))
                .into());
            }

            container_config
                .process_mut()
                .user_mut()
                .add_additional_gids(gid);
        }

        // Set the gid on /dev/pts
        for mount in container_config.mounts_mut() {
            if mount.typ() != "devpts" {
                continue;
            }

            mount.add_options(format!("gid={}", process_gid));
        }

        // container_config.process_mut().set_terminal(true);

        for arg in task.spec.args() {
            container_config.process_mut().add_args(arg.clone());
        }
        for val in task.spec.env() {
            container_config.process_mut().add_env(val.clone());
        }

        container_config.process_mut().set_cwd(task.spec.cwd());

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
                    let blob_dir = self.blob_data_dir().join(blob_id);

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
                    let dir = self.persistent_data_dir().join(name);
                    let dir_str = dir.to_str().unwrap().to_string();

                    let volume_gid;

                    if dir.exists().await {
                        // TODO: If the volume was partially created but the permissions were not
                        // set, then this may raise an error.

                        let gid = dir.metadata().await?.gid();

                        // TODO: Keep a separate record of the gids assigned to each volume and
                        // verify that they haven't changed.
                        // I'm not sure if it's a security risk if a container could change the
                        // group on a volume and then get an additional_gid when it is remounted.
                        if gid < self.shared.context.container_gids.start_id
                            || gid
                                >= (self.shared.context.container_gids.start_id
                                    + self.shared.context.container_gids.count)
                        {
                            return Err(format_err!(
                                "Persistent volume belows to unmanaged group: {}",
                                gid
                            ));
                        }

                        volume_gid = gid;
                    } else {
                        volume_gid = state_inner.next_gid;
                        if volume_gid == state_inner.next_uid {
                            // Keep them aligned to simplify debugging.
                            state_inner.next_uid += 1;
                        }

                        state_inner.next_gid += 1;

                        common::async_std::fs::create_dir_all(&dir).await?;
                        chown(dir_str.as_str(), None, Some(Gid::from_raw(volume_gid)))?;

                        let mut perms = dir.metadata().await?.permissions();
                        perms.set_mode(0o770 | libc::S_ISGID);
                        common::async_std::fs::set_permissions(&dir, perms).await?;
                    }

                    container_config
                        .process_mut()
                        .user_mut()
                        .add_additional_gids(volume_gid);

                    mount.add_options("bind".into());
                    mount.set_source(dir_str);
                }
                TaskSpec_VolumeSourceCase::BuildTarget(_) => {
                    return Err(rpc::Status::invalid_argument(
                        "Build target volumes should converted locally first",
                    )
                    .into());
                }
                TaskSpec_VolumeSourceCase::Unknown => {
                    return Err(
                        rpc::Status::invalid_argument("No source configured for volume").into(),
                    );
                }
            }

            container_config.add_mounts(mount);
        }

        for device in task.spec.devices() {
            // TODO: Implement
            // - exclusive locks
            // - min and max quantity of each device
            // - re-mounting dynamically on hotplugs.
            // - custom destination path

            match device.source().source_case() {
                DeviceSourceSourceCase::Usb(selector) => {
                    let devices = self.shared.usb_context.enumerate_devices().await?;
                    let mut num_mounted = 0;
                    for dev in devices {
                        let desc = dev.device_descriptor()?;
                        if desc.idVendor == selector.vendor() as u16
                            && desc.idProduct == selector.product() as u16
                        {
                            let mut mount = ContainerMount::default();
                            mount.set_source(dev.devfs_path().to_str().unwrap());
                            mount.set_destination(dev.devfs_path().to_str().unwrap());
                            mount.add_options("bind".into());
                            container_config.add_mounts(mount);

                            let mut mount = ContainerMount::default();
                            mount.set_source(dev.sysfs_dir().to_str().unwrap());
                            mount.set_destination(dev.sysfs_dir().to_str().unwrap());
                            mount.add_options("bind".into());
                            container_config.add_mounts(mount);

                            num_mounted += 1;
                            break;
                        }
                    }

                    if num_mounted == 0 {
                        return Err(rpc::Status::invalid_argument(
                            "Insufficient number of USB devices available",
                        )
                        .into());
                    }
                }
                DeviceSourceSourceCase::Raw(path) => {
                    if !path.starts_with("/dev/") {
                        return Err(rpc::Status::invalid_argument(format!(
                            "Path does not reference a device: {}",
                            path
                        ))
                        .into());
                    }

                    let mut mount = ContainerMount::default();
                    mount.set_source(path.as_str());
                    mount.set_destination(path.as_str());
                    mount.add_options("bind".into());
                    container_config.add_mounts(mount);
                }
                DeviceSourceSourceCase::Unknown => {
                    return Err(
                        rpc::Status::invalid_argument("No source configured for device").into(),
                    );
                }
            }
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
        let timeout_duration =
            Duration::from_secs(self.shared.config.graceful_shutdown_timeout_secs() as u64);
        let timeout_sender = self.shared.event_channel.0.clone();
        let timeout_task = ChildTask::spawn(async move {
            common::async_std::task::sleep(timeout_duration).await;
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

    pub async fn start_task(&self, task_spec: &TaskSpec) -> Result<()> {
        let mut state_guard = self.shared.state.lock().await;
        let state = &mut *state_guard;

        // TODO: Eventually delete non-persistent tasks.
        tasks_table::put_task(&self.shared.db, task_spec).await?;

        let existing_task = state
            .tasks
            .iter_mut()
            .find(|t| t.spec.name() == task_spec.name());

        let task = {
            if let Some(task) = existing_task {
                // TODO: Consider preserving the previous task_spec until the new one is added.
                task.spec = task_spec.clone();
                task
            } else {
                state.tasks.push(Task {
                    spec: task_spec.clone(),
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
        self.start_task(request.task_spec()).await
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

        // TODO: Immediately after a node is started, this may return a not found error
        // as the file wouldn't have been written to disk yet.

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
        let blob_dir = self.blob_data_dir().join(first_part.id());

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
