mod blob_store;
pub mod main;
pub mod shadow;
mod tasks_table;

use std::collections::{HashMap, HashSet};
use std::convert::TryFrom;
use std::os::unix::prelude::{MetadataExt, PermissionsExt};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use builder::proto::bundle::BundleSpec;
use common::async_std::channel;
use common::async_std::path::{Path, PathBuf};
use common::async_std::sync::Mutex;
use common::errors::*;
use common::eventually::Eventually;
use common::task::ChildTask;
use crypto::random::RngExt;
use datastore::meta::client::{MetastoreClient, MetastoreClientInterface, MetastoreTransaction};
use http::backoff::*;
use nix::unistd::chown;
use nix::unistd::Gid;
use protobuf::Message;
use sstable::{EmbeddedDB, EmbeddedDBOptions};

use crate::meta::client::ClusterMetaClient;
use crate::meta::constants::*;
use crate::meta::GetClusterMetaTable;
use crate::node::blob_store::*;
use crate::node::shadow::*;
use crate::proto::blob::*;
use crate::proto::config::*;
use crate::proto::log::*;
use crate::proto::meta::*;
use crate::proto::node::*;
use crate::proto::node_service::*;
use crate::proto::task::*;
use crate::proto::task_event::*;
use crate::runtime::ContainerRuntime;

struct Task {
    /// Spec that was used to start this task.
    spec: TaskSpec,

    revision: u64,

    /// Id of the most recent container running this task.
    container_id: Option<String>,

    state: TaskState,

    start_backoff: ExponentialBackoff,

    /// The task was recently created or updated so we are waiting for the task
    /// to be started using the latest TaskSpec.
    ///
    /// Will be reset to false once we have entired the Starting|Running state.
    pending_update: Option<StartTaskRequest>,

    permanent_stop: bool,

    /// Leases for all blobs in use by this task when running.
    /// This is set when transitioning to the Running state and cleared when
    /// entering the Terminal state.
    blob_leases: Vec<BlobLease>,
}

enum TaskState {
    /// The task hasn't yet been scheduled to start running.
    ///
    /// This may be because the task was only just recently added or it is
    /// missing some resources/dependencies needed to run. By default, if there
    /// are missing resources, we will wait until the resources become available
    /// and then start running the task.
    ///
    /// TODO: In this state, enumerate all missing requirements
    Pending {
        /// Partial set of requirements needed by this task which aren't
        /// currently available.
        missing_requirements: ResourceSet,
    },

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
    RestartBackoff {
        timer_id: usize,
        timeout_task: ChildTask,
    },

    /// The container has exited and there is no plan to restart it.
    ///
    /// TODO: How do we determine if the
    Terminal, /*  {
               *     state: TaskTerminalState
               * } */
}

enum TaskTerminalState {
    /// This was a one-off task (with restart_policy set to something other than
    /// ALWAYS|UNKNOWN) and it completed with a successful exit code (of 0).
    Successful,

    /// This task was stopped before it completed its intended number of
    /// attempts.
    ///
    /// - If a task is killed gracefully with a signal like SIGINT but exits
    ///   with a code of 0, this is considered an Abort instead of a Success.
    /// - If a task had to be force killed because it was not responding, it is
    ///   considered a failure and will have a Failed terminal state.
    Aborted,

    Failed,
}

#[derive(Default)]
struct ResourceSet {
    /// Set of blob ids needed.
    blobs: HashSet<String>,
}

impl ResourceSet {
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }
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

/*
Node start up procedure:
- Generate the id
- Start any persistent containers that don't require metastore knowledge
- Connect to the meta store and update our own ip address
- Start normal containers with:
    - CLUSTER_NODE_ID=1234
    - CLUSTER_META_GROUP_ID=
    - CLUSTER
-

Have all persistent nodes marked as pending:
- Until we know the

Start up immediately.
=> Have dependencies
    =>

TODO: Should support one-off tasks which only run once without re-starting on failures (if they were successful?)
*/

enum NodeEvent {
    /// Triggered by the internal container runtime whenever the container
    /// running a task has changed in state.
    ContainerStateChange {
        container_id: String,
    },

    /// TODO: Remove this and use a TaskResultBundle instead.
    ContainerRuntimeEnded(Result<()>),

    ///
    /// Triggered by the blob fetcher task.
    BlobAvailable {
        blob_id: String,
    },

    StopTimeout {
        task_name: String,
        timer_id: usize,
    },

    /// We have waited long enough to re-start a task in the RestartBackoff
    /// state.
    StartBackoffTimeout {
        task_name: String,
        timer_id: usize,
    },
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

    /// Address at which the node can be reached by other nodes.
    /// This should contain the port on which the Node service will be started.
    ///
    /// e.g. '10.1.0.123:10400'
    pub local_address: String,
}

#[derive(Clone)]
pub struct Node {
    inner: NodeInner,
}

/// Split out from Node to make the service implementations private. Users
/// should add RPC services with add_services().
#[derive(Clone)]
struct NodeInner {
    shared: Arc<NodeShared>,
}

struct NodeShared {
    id: u64,

    context: NodeContext,
    config: NodeConfig,

    db: Arc<EmbeddedDB>,

    blobs: BlobStore,

    runtime: Arc<ContainerRuntime>,
    event_channel: (channel::Sender<NodeEvent>, channel::Receiver<NodeEvent>),
    state: Mutex<NodeState>,

    usb_context: usb::Context,

    last_timer_id: AtomicUsize,

    /// Available once we have connected and registered our node in the meta
    /// store.
    meta_client: Eventually<ClusterMetaClient>,

    /// Timestamp (in unix micros) of the last event we've recorded. This is
    /// used to ensure that all recorded events use a monotonic timestamp (at
    /// least since the last node reboot).
    ///
    /// TODO: Ensure monotonic timestamps even between node restarts.
    last_event_timestamp: Mutex<u64>,
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

    /// Tasks used to retrieve blobs from other servers.
    ///
    /// TODO: Need persistent backoff.
    /// TODO: If all tasks requiring a blob are removed, stop the fetcher task.
    blob_fetchers: HashMap<String, ChildTask>,
}

impl Node {
    pub async fn create(context: &NodeContext, config: &NodeConfig) -> Result<Self> {
        let mut db_options = EmbeddedDBOptions::default();
        db_options.create_if_missing = true;

        let db =
            Arc::new(EmbeddedDB::open(Path::new(config.data_dir()).join("db"), db_options).await?);

        let id = match tasks_table::get_saved_node_id(&db).await? {
            Some(id) => id,
            None => {
                let id = if config.bootstrap_id_from_machine_id() {
                    let machine_id = common::hex::decode(
                        common::async_std::fs::read_to_string("/etc/machine-id")
                            .await?
                            .trim(),
                    )?;

                    if machine_id.len() < 8 {
                        return Err(err_msg("Expected machine id to have at least 8 bytes"));
                    }

                    u64::from_be_bytes(*array_ref![machine_id, 0, 8])
                } else {
                    return Err(err_msg("Node not initialized with an id"));
                };

                tasks_table::set_saved_node_id(&db, id).await?;

                id
            }
        };

        println!("Node id: {:08x}", id);

        let inner = NodeStateInner {
            container_id_to_task_name: HashMap::new(),
            // TODO: Preserve these across reboots and attempt to not re-use ids already
            // referenced in the file system.
            next_uid: context.container_uids.start_id,
            next_gid: context.container_gids.start_id,
            mounted_paths: HashMap::new(),
            blob_fetchers: HashMap::new(),
        };

        let usb_context = usb::Context::create()?;

        let blobs =
            BlobStore::create(Path::new(config.data_dir()).join("blob"), db.clone()).await?;

        let last_event_timestamp = {
            let last_db_time = tasks_table::get_events_timestamp(&db).await?.unwrap_or(0);
            let current_time = NodeInner::current_system_timestamp();
            core::cmp::max(last_db_time, current_time)
        };

        let runtime = ContainerRuntime::create(Path::new(config.data_dir()).join("run")).await?;
        let inst = NodeInner {
            shared: Arc::new(NodeShared {
                id,
                context: context.clone(),
                config: config.clone(),
                db,
                blobs,
                runtime,
                state: Mutex::new(NodeState {
                    tasks: vec![],
                    inner,
                }),
                event_channel: channel::unbounded(),
                last_timer_id: AtomicUsize::new(0),
                usb_context,
                meta_client: Eventually::new(),
                last_event_timestamp: Mutex::new(last_event_timestamp),
            }),
        };

        let all_tasks = tasks_table::list_tasks(&inst.shared.db).await?;
        for task_meta in all_tasks {
            if !task_meta.spec().persistent() {
                // TODO: Also add non-persistent tasks in a terminal state.
                continue;
            }

            match task_meta.state() {
                TaskMetadata_State::STARTING | TaskMetadata_State::READY => {
                    // Should work
                }
                _ => continue,
            }

            // TODO: Don't allow the failure of this to block node startup.
            // We don't want the
            // TODO: Provide more gurantees that any tasks that are persisted will actually
            // be start-able.
            let mut req = StartTaskRequest::default();
            req.set_spec(task_meta.spec().clone());
            req.set_revision(task_meta.revision());

            if let Err(e) = inst.start_task(&req).await {
                // TOOD: This should probably trigger a real error now that we isolate the start
                // request.
                eprintln!("Persistent task failed to start: {}", e);
            }
        }

        // TODO: Ideally this should run after the server is started so that we can mark
        // ourselves as available at the right time.

        Ok(Self { inner: inst })
    }

    pub fn run(&self) -> impl std::future::Future<Output = Result<()>> {
        self.clone().inner.run_impl()
    }

    pub fn add_services(&self, rpc_server: &mut rpc::Http2Server) -> Result<()> {
        rpc_server.add_service(self.inner.clone().into_service())?;
        rpc_server.add_service(self.inner.shared.blobs.clone().into_service())?;
        Ok(())
    }
}

impl NodeInner {
    // TODO: Implement graceful node shutdown which terminates all the inner nodes.
    // ^ Also flush any pending data to disk.

    async fn run_impl(self) -> Result<()> {
        let mut task_bundle = common::bundle::TaskResultBundle::new();

        // This task runs the internal container runtime.
        task_bundle.add("cluster::ContainerRuntime::run", {
            let runtime = self.shared.runtime.clone();
            let sender = self.shared.event_channel.0.clone();

            async move {
                let result = runtime.run().await;
                let _ = sender.send(NodeEvent::ContainerRuntimeEnded(result)).await;
                Ok(())
            }
        });

        // This task forwards container events from the container runtime to the node
        // event loop.
        task_bundle.add("cluster::Node::runtime_listener", {
            let receiver = self.shared.runtime.add_event_listener().await;
            let sender = self.shared.event_channel.0.clone();

            async move {
                while let Ok(container_id) = receiver.recv().await {
                    let _ = sender
                        .send(NodeEvent::ContainerStateChange { container_id })
                        .await;
                }

                Ok(())
            }
        });

        task_bundle.add(
            "cluster::Node::run_node_registration",
            self.clone().run_node_registration(),
        );

        task_bundle.add("cluster::Node::run_event_loop", self.run_event_loop());

        task_bundle.join().await
    }

    /// Registers the node in the cluster.
    ///
    /// Internally this tries to contact the metastore instance and update our
    /// entry. Because the metastore may be running on this node, this will
    /// aggresively retry and backoff until the metastore is found.
    ///
    /// TODO: Make this run after the RPC server has started.
    async fn run_node_registration(self) -> Result<()> {
        let zone = self.shared.config.zone();
        if zone.is_empty() {
            println!("Node running outside of cluster zone");
            return Ok(());
        }

        let start_time = SystemTime::now();

        let meta_client = ClusterMetaClient::create(zone).await?;

        // Perform initial update of our node entry.
        run_transaction!(&meta_client, txn, {
            let node_table = txn.cluster_table::<NodeMetadata>();
            let mut node_meta = node_table.get(&self.shared.id).await?.unwrap_or_default();
            node_meta.set_id(self.shared.id);
            node_meta.set_address(&self.shared.context.local_address);
            node_meta.set_start_time(start_time);
            node_meta.set_last_seen(SystemTime::now());
            node_meta.set_zone(zone);
            if node_meta.state() == NodeMetadata_State::UNKNOWN {
                node_meta.set_state(NodeMetadata_State::NEW);
            }
            node_meta
                .set_allocatable_port_range(self.shared.config.allocatable_port_range().clone());
            node_table.put(&node_meta).await?;
        });

        println!("Node registered in metastore!");

        self.shared.meta_client.set(meta_client).await?;

        // Periodically mark this node as still active.
        // TODO: Allow this is fail and continue to retry.
        loop {
            let meta_client = self.shared.meta_client.get().await;

            let node_meta = run_transaction!(meta_client, txn, {
                let node_table = txn.cluster_table::<NodeMetadata>();
                let mut node_meta = node_table.get(&self.shared.id).await?.unwrap();
                node_meta.set_last_seen(SystemTime::now());
                node_table.put(&node_meta).await?;
                node_meta
            });

            if node_meta.state() != NodeMetadata_State::NEW {
                self.reconcile_tasks().await?;
            }

            common::async_std::task::sleep(NODE_HEARTBEAT_INTERVAL).await;
        }

        Ok(())
    }

    /// Diffs the set of tasks on the current node with these specified for this
    /// node in the metastore (and applied the differences to this node).
    ///
    /// TODO: Run this with it's own backoff loop.
    /// TODO: Make sure that all external requests have deadlines.
    async fn reconcile_tasks(&self) -> Result<()> {
        let meta_client = self.shared.meta_client.get().await;

        let intended_tasks = meta_client
            .cluster_table::<TaskMetadata>()
            .list_by_node(self.shared.id)
            .await?;

        let existing_tasks = self.list_tasks_impl().await?;

        let existing_tasks_list = self.list_tasks_impl().await?;
        let mut existing_tasks = HashMap::new();
        for task in existing_tasks_list.tasks() {
            // TODO: Skip permanently stopped tasks.
            existing_tasks.insert(task.spec().name(), task);
        }

        for task in intended_tasks {
            // TODO: Split any tasks which are in the STOPPED | STOP state.

            if let Some(existing_task) = existing_tasks.remove(task.spec().name()) {
                if existing_task.revision() == task.revision() {
                    continue;
                }
            }

            // TODO: If this fails, record it in the TaskMetadata.
            let mut req = StartTaskRequest::default();
            req.set_spec(task.spec().clone());
            req.set_revision(task.revision());
            self.start_task(&req).await?;
        }

        for (_, task) in existing_tasks {
            // TODO: Stop task.
            self.stop_task(task.spec().name()).await?;
        }

        // TODO: When tasks reach their terminal state, record that in the metastore?

        Ok(())
    }

    async fn run_event_loop(self) -> Result<()> {
        loop {
            let event = self.shared.event_channel.1.recv().await?;
            match event {
                NodeEvent::ContainerStateChange { container_id } => {
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

                    let container_meta = self
                        .shared
                        .runtime
                        .get_container(&container_id)
                        .await
                        .ok_or_else(|| err_msg("Faield to find container"))?;

                    // Currently this is the only state change type implemented in the runtime.
                    if container_meta.state() != ContainerState::Stopped {
                        return Err(err_msg(
                            "Expected state changes only with stopped containers",
                        ));
                    }

                    let mut event = TaskEvent::default();
                    event.set_task_name(task.spec.name());
                    event.set_task_revision(task.revision);
                    event.set_container_id(&container_id);
                    event
                        .stopped_mut()
                        .set_status(container_meta.status().clone());
                    self.record_event(event).await?;

                    // No longer running, so clear the container id
                    task.container_id = None;

                    if task.pending_update.is_some() {
                        self.transition_task_to_running(&mut state.inner, task)
                            .await?;
                    } else {
                        self.transition_task_to_backoff(task).await;
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

                        let mut event = TaskEvent::default();
                        event.set_task_name(task.spec.name());
                        event.set_task_revision(task.revision);
                        event.set_container_id(container_id);
                        event.stopping_mut().set_force(true);
                        self.record_event(event).await?;

                        self.shared
                            .runtime
                            .kill_container(container_id, nix::sys::signal::Signal::SIGKILL)
                            .await?;

                        task.state = TaskState::ForceStopping;
                    }
                }
                NodeEvent::BlobAvailable { blob_id } => {
                    // When a blob is available, we want to check all pending tasks to see if that
                    // allows us to start running it.

                    let mut state_guard = self.shared.state.lock().await;
                    let state = &mut *state_guard;

                    // We no longer need to be fetching the blob.
                    state.inner.blob_fetchers.remove(&blob_id);

                    for task in &mut state.tasks {
                        if let TaskState::Pending {
                            missing_requirements,
                        } = &mut task.state
                        {
                            missing_requirements.blobs.remove(&blob_id);

                            if missing_requirements.is_empty() {
                                self.transition_task_to_running(&mut state.inner, task)
                                    .await;
                            }
                        }
                    }
                }
                NodeEvent::StartBackoffTimeout {
                    task_name,
                    timer_id: event_timer_id,
                } => {
                    let mut state_guard = self.shared.state.lock().await;
                    let state = &mut *state_guard;

                    let task = match state.tasks.iter_mut().find(|t| t.spec.name() == task_name) {
                        Some(t) => t,
                        None => {
                            // Most likely a race condition with the timer event being processed
                            // after the task was deleted.
                            continue;
                        }
                    };

                    let mut should_start = false;
                    if let TaskState::RestartBackoff { timer_id, .. } = &task.state {
                        if *timer_id == event_timer_id {
                            should_start = true;
                        }
                    }

                    if should_start {
                        self.transition_task_to_running(&mut state.inner, task)
                            .await;
                    }
                }
                NodeEvent::ContainerRuntimeEnded(result) => {
                    if result.is_ok() {
                        return Err(err_msg("Container runtime ended early"));
                    }

                    return result;
                }
            }
        }
    }

    fn persistent_data_dir(&self) -> PathBuf {
        Path::new(self.shared.config.data_dir()).join("persistent")
    }

    async fn list_tasks_impl(&self) -> Result<ListTasksResponse> {
        let state = self.shared.state.lock().await;
        let mut out = ListTasksResponse::default();
        for task in &state.tasks {
            let mut proto = TaskProto::default();
            proto.set_spec(task.spec.clone());
            proto.set_revision(task.revision);
            if let Some(pending_update) = &task.pending_update {
                proto.set_pending_update(pending_update.clone());
            }
            if let Some(id) = &task.container_id {
                if let Some(meta) = self.shared.runtime.get_container(id.as_str()).await {
                    proto.set_container(meta);
                }
            }

            proto.set_state(match &task.state {
                TaskState::Pending {
                    missing_requirements,
                } => TaskStateProto::PENDING,
                TaskState::Running => TaskStateProto::RUNNING,
                TaskState::Stopping {
                    timer_id,
                    timeout_task,
                } => TaskStateProto::STOPPING,
                TaskState::ForceStopping => TaskStateProto::FORCE_STOPPING,
                TaskState::RestartBackoff {
                    timer_id,
                    timeout_task,
                } => TaskStateProto::RESTART_BACKOFF,
                TaskState::Terminal => TaskStateProto::TERMINAL,
            });

            out.add_tasks(proto);
        }

        Ok(out)
    }

    /// Tries to transition the task to the Running state.
    /// When this is called, we assume that we are currently not running any
    /// containers and if any backoff was required, it has already been
    /// waited.
    ///
    /// NOTE: If this function returns an Error, it should be considered fatal.
    /// Most partial task specific failures should be done in
    /// transition_task_to_running_impl.
    async fn transition_task_to_running(
        &self,
        state_inner: &mut NodeStateInner,
        task: &mut Task,
    ) -> Result<()> {
        if let Some(req) = task.pending_update.take() {
            task.revision = req.revision();
            task.spec = req.spec().clone();
        }

        if task.container_id.is_some() {
            return Err(err_msg("Task still has an old container_id while starting"));
        }

        if let Err(e) = self
            .transition_task_to_running_impl(state_inner, task)
            .await
        {
            // TODO: Can we differentiate between failures caused by the node and failures
            // caused by the task's specification? (to know which ones should and shouldn't
            // be retried)

            let status = {
                if let Some(e) = e.downcast_ref::<rpc::Status>() {
                    e.to_proto()
                } else {
                    rpc::Status::unknown(format!("{}", e)).to_proto()
                }
            };

            let mut event = TaskEvent::default();
            event.set_task_name(task.spec.name());
            event.set_task_revision(task.revision);
            event.start_failure_mut().set_status(status);

            self.record_event(event).await?;

            self.transition_task_to_backoff(task).await;
        }

        Ok(())
    }

    async fn transition_task_to_running_impl(
        &self,
        state_inner: &mut NodeStateInner,
        task: &mut Task,
    ) -> Result<()> {
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

        {
            container_config
                .process_mut()
                .add_env(format!("{}={}", NODE_ID_ENV_VAR, self.shared.id));
            container_config.process_mut().add_env(format!(
                "{}={}",
                TASK_NAME_ENV_VAR,
                task.spec.name()
            ));
            container_config.process_mut().add_env(format!(
                "{}={}",
                ZONE_ENV_VAR,
                self.shared.config.zone()
            ));
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

        let mut missing_requirements = ResourceSet::default();

        let mut blob_leases = vec![];

        for volume in task.spec.volumes() {
            let mut mount = ContainerMount::default();
            mount.set_destination(format!("/volumes/{}", volume.name()));

            match volume.source_case() {
                TaskSpec_VolumeSourceCase::Bundle(bundle) => {
                    let blob_id = self.select_bundle_blob(bundle)?;

                    let blob_lease = match self.shared.blobs.read_lease(blob_id.as_str()).await {
                        Ok(v) => v,
                        Err(ReadBlobError::BeingWritten) | Err(ReadBlobError::NotFound) => {
                            self.start_fetching_blob(state_inner, blob_id.as_str())
                                .await;
                            missing_requirements.blobs.insert(blob_id.clone());
                            continue;
                        }
                    };

                    mount.set_source(blob_lease.extracted_dir().to_str().unwrap());
                    mount.add_options("bind".into());
                    mount.add_options("ro".into());
                    blob_leases.push(blob_lease);
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

        if !missing_requirements.is_empty() {
            // TODO: Log this as an event.

            task.state = TaskState::Pending {
                missing_requirements,
            };
            return Ok(());
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

        task.blob_leases = blob_leases;
        task.container_id = Some(container_id.clone());
        task.state = TaskState::Running;

        let mut event = TaskEvent::default();
        event.set_task_name(task.spec.name());
        event.set_task_revision(task.revision);
        event.set_container_id(&container_id);
        event.started_mut();
        self.record_event(event).await?;

        Ok(())
    }

    fn select_bundle_blob(&self, bundle: &BundleSpec) -> Result<String> {
        let platform = builder::current_platform()?;

        for variant in bundle.variants() {
            if variant.platform() == &platform {
                return Ok(variant.blob().id().to_string());
            }
        }

        Err(rpc::Status::invalid_argument(format!(
            "No bundle variant matches platform: {:?}",
            platform
        ))
        .into())
    }

    async fn start_fetching_blob(&self, state_inner: &mut NodeStateInner, blob_id: &str) {
        // TODO: Limit max blob fetching parallelism.
        if !state_inner.blob_fetchers.contains_key(blob_id) {
            // TODO: Verify that fetchers are always cleaned up upon completion.
            state_inner.blob_fetchers.insert(
                blob_id.to_string(),
                ChildTask::spawn(self.clone().fetch_blob(blob_id.to_string())),
            );
        }
    }

    /// Arguments:
    /// - task:
    /// - successful: true if the task has been run and exited with a successful
    ///   status code.
    async fn transition_task_to_backoff(&self, task: &mut Task /* , successful: bool */) {
        // let should_retry = match task.spec.restart_policy() {
        //     TaskSpec_RestartPolicy::UNKNOWN | TaskSpec_RestartPolicy::ALWAYS => true,
        //     TaskSpec_RestartPolicy::NEVER => false,
        //     TaskSpec_RestartPolicy::ON_FAILURE => !successful,
        // };

        // if !should_retry {
        //     self.transition_task_to_terminal(task);
        //     return;
        // }

        // TODO: Check the restart policy to see if we should

        if task.permanent_stop {
            self.transition_task_to_terminal(task);
            return;
        }

        // NOTE: This is intentionally false and not using the 'successful' bool.
        task.start_backoff.end_attempt(false);

        match task.start_backoff.start_attempt() {
            ExponentialBackoffResult::Start => {
                // NOTE: This should never happen as we marked the attempt as failing.
                // TODO: consider waiting for a minimum amount of time in this case.
                panic!("No backoff time for container task")
            }
            ExponentialBackoffResult::StartAfter(wait_time) => {
                // TODO: Deduplicate this timer code.
                // TODO: Instead use the task id of the child task.
                let timer_id = self
                    .shared
                    .last_timer_id
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

                let task_name = task.spec.name().to_string();
                let timeout_sender = self.shared.event_channel.0.clone();
                let timeout_task = ChildTask::spawn(async move {
                    common::async_std::task::sleep(wait_time).await;
                    let _ = timeout_sender
                        .send(NodeEvent::StartBackoffTimeout {
                            task_name,
                            timer_id,
                        })
                        .await;
                });

                task.state = TaskState::RestartBackoff {
                    timer_id,
                    timeout_task,
                };
            }
            ExponentialBackoffResult::Stop => {
                self.transition_task_to_terminal(task);
            }
        }
    }

    fn transition_task_to_terminal(&self, task: &mut Task) {
        task.state = TaskState::Terminal;
        task.blob_leases.clear();
    }

    async fn fetch_blob(self, blob_id: String) {
        let mut backoff = ExponentialBackoff::new(ExponentialBackoffOptions {
            base_duration: Duration::from_secs(10),
            jitter_duration: Duration::from_secs(10),
            max_duration: Duration::from_secs(2 * 60),
            cooldown_duration: Duration::from_secs(4 * 60),
            max_num_attempts: 0,
        });

        loop {
            match backoff.start_attempt() {
                ExponentialBackoffResult::Start => {}
                ExponentialBackoffResult::StartAfter(wait_time) => {
                    common::async_std::task::sleep(wait_time).await
                }
                ExponentialBackoffResult::Stop => {
                    // TODO: If all attempts fail, then mark all pending tasks as failed.
                    return;
                }
            }

            match self.fetch_blob_once(&blob_id).await {
                Ok(()) => {
                    self.shared
                        .event_channel
                        .0
                        .send(NodeEvent::BlobAvailable { blob_id: blob_id })
                        .await;
                    return;
                }
                Err(e) => {
                    eprintln!("Failed to fetch blob: {}", e);
                    continue;
                }
            }
        }
    }

    async fn fetch_blob_once(&self, blob_id: &str) -> Result<()> {
        // Check if we already have the blob.
        // This would mainly happen if the user recently uploaded the blob directly to
        // this server. TODO: Have the BlobStore object directly emit events to
        // the Node
        if let Ok(_) = self.shared.blobs.read_lease(blob_id).await {
            return Ok(());
        }

        let meta_client = self.shared.meta_client.get().await;

        let blob_meta = meta_client
            .cluster_table::<BlobMetadata>()
            .get(blob_id)
            .await?
            .ok_or_else(|| err_msg("No such blob"))?;

        let uploaded_replicas = blob_meta
            .replicas()
            .iter()
            .filter(|replica| replica.uploaded())
            .collect::<Vec<_>>();

        if uploaded_replicas.is_empty() {
            return Err(err_msg("No replicas for blob"));
        }

        // TODO: Don't try fetching the blob from ourselves.
        let remote_node_id = crypto::random::clocked_rng()
            .choose(&uploaded_replicas)
            .node_id();

        // TODO: Exclude nodes not marked as ready recently.
        let remote_node_meta = meta_client
            .cluster_table::<NodeMetadata>()
            .get(&remote_node_id)
            .await?
            .ok_or_else(|| err_msg("No such node"))?;

        let client = rpc::Http2Channel::create(http::ClientOptions::try_from(
            format!("http://{}", remote_node_meta.address()).as_str(),
        )?)?;

        let stub = BlobStoreStub::new(Arc::new(client));

        let request_context = rpc::ClientRequestContext::default();

        let mut request = BlobDownloadRequest::default();
        request.set_blob_id(blob_id);

        let mut res = stub.Download(&request_context, &request).await;

        let first_part = match res.recv().await {
            Some(v) => v,
            None => {
                res.finish().await?;
                return Err(err_msg("Didn't get first part to Download response"));
            }
        };

        let mut blob_writer = match self.shared.blobs.new_writer(&first_part.spec()).await? {
            Ok(v) => v,
            Err(_) => {
                return Err(err_msg("Failed to acquire blob writer"));
            }
        };

        blob_writer.write(first_part.data()).await?;

        while let Some(part) = res.recv().await {
            blob_writer.write(part.data()).await?;
        }

        res.finish().await?;

        blob_writer.finish().await?;

        Ok(())
    }

    async fn transition_task_to_stopping(&self, task: &mut Task) -> Result<()> {
        let container_id = task.container_id.as_ref().unwrap();

        let mut event = TaskEvent::default();
        event.set_task_name(task.spec.name());
        event.set_task_revision(task.revision);
        event.set_container_id(container_id);
        event.stopping_mut().set_force(false);
        self.record_event(event).await?;

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

    pub async fn start_task(&self, request: &StartTaskRequest) -> Result<()> {
        let mut state_guard = self.shared.state.lock().await;
        let state = &mut *state_guard;

        // TODO: Consider only storing this once the task successfully starts up.
        // TODO: Eventually delete non-persistent tasks.
        // TODO: Once a task has failed, don't restart it on node re-boots.
        let mut meta = TaskMetadata::default();
        meta.set_spec(request.spec().clone());
        meta.set_state(TaskMetadata_State::STARTING);
        meta.set_revision(request.revision());
        tasks_table::put_task(&self.shared.db, &meta).await?;

        let existing_task = state
            .tasks
            .iter_mut()
            .find(|t| t.spec.name() == request.spec().name());

        let task = {
            if let Some(task) = existing_task {
                task.permanent_stop = false;
                task.start_backoff.reset();
                task.pending_update = Some(request.clone());
                task
            } else {
                state.tasks.push(Task {
                    spec: request.spec().clone(),
                    revision: request.revision(),
                    container_id: None,
                    state: TaskState::Pending {
                        missing_requirements: ResourceSet::default(),
                    },
                    pending_update: None,
                    permanent_stop: false,
                    blob_leases: vec![],
                    start_backoff: ExponentialBackoff::new(ExponentialBackoffOptions {
                        base_duration: Duration::from_secs(10),
                        jitter_duration: Duration::from_secs(5),
                        max_duration: Duration::from_secs(5 * 60), // 5 minutes
                        cooldown_duration: Duration::from_secs(10 * 60), // 10 minutes
                        max_num_attempts: 8,
                    }),
                });
                state.tasks.last_mut().unwrap()
            }
        };

        match &task.state {
            TaskState::Pending { .. } | TaskState::RestartBackoff { .. } | TaskState::Terminal => {
                self.transition_task_to_running(&mut state.inner, task)
                    .await?;
            }
            TaskState::Running => {
                self.transition_task_to_stopping(task).await?;
            }
            TaskState::Stopping { .. } | TaskState::ForceStopping => {
                // We don't need to do anything. Once the container finishes
                // stopping, the new container will be brought
                // up.
            }
        }

        Ok(())
    }

    /// TODO: Should we have this compare to the revision of the task?
    pub async fn stop_task(&self, name: &str) -> Result<()> {
        let mut state_guard = self.shared.state.lock().await;
        let state = &mut *state_guard;

        let task = state
            .tasks
            .iter_mut()
            .find(|t| t.spec.name() == name)
            .ok_or_else(|| rpc::Status::not_found("Task not found"))?;

        task.pending_update = None;
        task.permanent_stop = true;

        let mut meta = TaskMetadata::default();
        meta.set_state(TaskMetadata_State::STOPPING);
        tasks_table::put_task(&self.shared.db, &meta).await?;

        match &task.state {
            TaskState::Pending { .. } | TaskState::RestartBackoff { .. } | TaskState::Running => {
                self.transition_task_to_stopping(task).await?;
            }
            TaskState::Stopping { .. } | TaskState::ForceStopping | TaskState::Terminal => {
                // Nothing to do.
            }
        }

        Ok(())
    }

    fn current_system_timestamp() -> u64 {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    async fn record_event(&self, mut event: TaskEvent) -> Result<()> {
        eprintln!("Event: {:?}", event);

        let mut time = self.shared.last_event_timestamp.lock().await;
        *time = core::cmp::max(*time + 1, Self::current_system_timestamp());
        event.set_timestamp(*time);
        drop(time);

        tasks_table::put_task_event(self.shared.db.as_ref(), &event).await
    }
}

#[async_trait]
impl ContainerNodeService for NodeInner {
    async fn Identity(
        &self,
        request: rpc::ServerRequest<google::proto::empty::Empty>,
        response: &mut rpc::ServerResponse<NodeMetadata>,
    ) -> Result<()> {
        response.value.set_id(self.shared.id);
        response.value.set_zone(self.shared.config.zone());
        Ok(())
    }

    // rpc ListTasks (ListTasksRequest) returns (ListTasksResponse);

    async fn ListTasks(
        &self,
        request: rpc::ServerRequest<ListTasksRequest>,
        response: &mut rpc::ServerResponse<ListTasksResponse>,
    ) -> Result<()> {
        response.value = self.list_tasks_impl().await?;

        Ok(())
    }

    async fn ReplicateBlob(
        &self,
        request: rpc::ServerRequest<ReplicateBlobRequest>,
        response: &mut rpc::ServerResponse<google::proto::empty::Empty>,
    ) -> Result<()> {
        // Start the replication
        {
            let mut state = self.shared.state.lock().await;
            self.start_fetching_blob(&mut state.inner, request.blob_id())
                .await;
        }

        // TODO: Block for the replication to succeed or permanently fail?

        Ok(())
    }

    async fn StartTask(
        &self,
        request: rpc::ServerRequest<StartTaskRequest>,
        response: &mut rpc::ServerResponse<StartTaskResponse>,
    ) -> Result<()> {
        self.start_task(&request.value).await
    }

    // TODO: When the Node closes, we should kill all tasks that it has

    async fn GetLogs(
        &self,
        request: rpc::ServerRequest<LogRequest>,
        response: &mut rpc::ServerStreamResponse<LogEntry>,
    ) -> Result<()> {
        let container_id = {
            if request.attempt_id() != 0 {
                let attempt_event =
                    tasks_table::get_task_events(&self.shared.db, request.task_name())
                        .await?
                        .into_iter()
                        .find(|e| e.timestamp() == request.attempt_id())
                        .ok_or_else(|| {
                            rpc::Status::not_found("Failed to find attempt with given id")
                        })?;

                if !attempt_event.has_started() {
                    return Err(rpc::Status::invalid_argument(
                        "Attempt event is not a start event",
                    )
                    .into());
                }

                attempt_event.container_id().to_string()
            } else {
                // Default behavior is to look up the currently running container.

                let state = self.shared.state.lock().await;
                let task = state
                    .tasks
                    .iter()
                    .find(|t| t.spec.name() == request.task_name())
                    .ok_or_else(|| {
                        Error::from(rpc::Status::not_found(format!(
                            "No task found with name: {}",
                            request.task_name()
                        )))
                    })?;

                task.container_id.clone().ok_or_else(|| {
                    rpc::Status::invalid_argument("Container not currently running")
                })?
            }
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
        _response: &mut rpc::ServerResponse<google::proto::empty::Empty>,
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
                        Error::from(rpc::Status::not_found(format!(
                            "No task found with name: {}",
                            input.task_name()
                        )))
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

    async fn GetEvents(
        &self,
        request: rpc::ServerRequest<GetEventsRequest>,
        response: &mut rpc::ServerResponse<GetEventsResponse>,
    ) -> Result<()> {
        let events = tasks_table::get_task_events(&self.shared.db, request.task_name()).await?;
        for event in events {
            response.value.add_events(event);
        }

        Ok(())
    }
}
