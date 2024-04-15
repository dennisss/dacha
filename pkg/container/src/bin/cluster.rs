// Cluster Management CLI
// TODO: Combine all of the CLI utilities into this one.
/*
Aside from the 'bootstrap' command, all commands require

Testing:
    cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto

    cargo run --bin cluster -- start_worker --node_addr=127.0.0.1:10400 pkg/rpc_test/config/adder_server.worker

Next steps:
-

Testing with a single node cluster:
    cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto --zone=dev

    cargo run --bin cluster -- bootstrap --node_addr=127.0.0.1:10400

    CLUSTER_ZONE=dev cargo run --bin cluster -- list jobs

    CLUSTER_ZONE=dev cargo run --bin cluster -- start_job pkg/rpc_test/config/adder_server.job

    CLUSTER_ZONE=dev cargo run --bin adder_client -- --target=adder_server.job.local.cluster.internal

    CLUSTER_ZONE=dev cargo run --bin cluster -- log --worker_name=adder_server.256326fbfc425883

    <try modifying the adder_server job and rerunning the start_job / adder_client code to verify that we can update to the new revision>

    <try stopping and restarting the node. everything should still work>



Testing with a single node non-cluster:
    cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto

    cargo run --bin cluster -- start_worker pkg/rpc_test/config/adder_server.task --node_addr=127.0.0.1:10400

    cargo run --bin adder_client -- add 1 2 --target=127.0.0.1:30001

    cargo run --bin adder_client -- busy_loop 0.1 --target=127.0.0.1:30001

    cargo run --bin cluster -- list workers --node_addr=127.0.0.1:10400

*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;
#[macro_use]
extern crate file;

// TODO: Given this is only used for bootstrapping, consider refactoring out
// this dependency.
extern crate raft;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::str::FromStr;
use std::time::{Duration, SystemTime};
use std::{collections::HashSet, sync::Arc};

use builder::proto::{BlobFormat, BundleSpec};
use common::errors::*;
use common::failure::ResultExt;
use common::io::{Readable, Writeable};
use container::manager::Manager;
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use crypto::sip::SipHasher;
use datastore_meta_client::MetastoreClientInterface;
use executor::cancellation::AlreadyCancelledToken;
use executor::child_task::ChildTask;
use executor::JoinHandle;
use executor_multitask::ServiceResource;
use nix::{
    sys::termios::{tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags},
    unistd::isatty,
};
use protobuf::text::parse_text_proto;
use protobuf::text::ParseTextProto;
use protobuf::Message;
use raft::log::segmented_log::SegmentedLogOptions;
use raft::proto::Configuration_ServerRole;
use rpc::ClientRequestContext;

use cluster_client::meta::client::ClusterMetaClient;
use cluster_client::meta::*;
use container::{
    AllocateBlobsRequest, AllocateBlobsResponse, BlobMetadata, JobSpec, ListWorkersRequest,
    ManagerIntoService, ManagerStub, NodeMetadata, StartJobRequest,
    WorkerStateMetadata_ReportedState,
};
use container::{
    BlobStoreStub, ContainerNodeStub, JobMetadata, NodeMetadata_State, WorkerMetadata, WorkerSpec,
    WorkerSpec_Port, WorkerSpec_Volume, WorkerStateMetadata, WriteInputRequest, ZoneMetadata,
};

#[derive(Args)]
pub struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    /// Initializes a new cluster. This should only be called once when
    /// initially setting up a new set of nodes.
    ///
    /// Before this is run, there must already be at least one node machine
    /// running.
    #[arg(name = "bootstrap")]
    Bootstrap(BootstrapCommand),

    /// Re-builds all system cluster components (metastore, manager) and updates
    /// them in a running cluster.
    #[arg(name = "upgrade")]
    Upgrade(UpgradeCommand),

    /// Enumerate objects in the cluster (workers, )
    #[arg(name = "list")]
    List(ListCommand),

    #[arg(name = "start_job")]
    StartJob(StartJobCommand),

    /// Start a single worker directly on a node. This is mainly for cluster
    /// bootstrapping.
    #[arg(name = "start_worker")]
    StartWorker(StartWorkerCommand),

    #[arg(name = "events")]
    Events(EventsCommand),

    /// Retrieve the log (stdout/stderr) outputs of a worker.
    #[arg(name = "log")]
    Log(LogCommand),
}

#[derive(Args)]
struct BootstrapCommand {
    node_addr: String,

    /// For the purposes of initializing the cluster, a local metastore instance
    /// will be brought up before one is running in the cluster.
    ///
    /// This will be the port used on the local machine to server requests to
    /// this instance.
    #[arg(default = 4000)]
    local_metastore_port: u16,
}

#[derive(Args)]
struct UpgradeCommand {}

#[derive(Args)]
struct ListCommand {
    /// What type of objects to enumerate. If not specified, we will enumerate
    /// all objects.
    #[arg(positional)]
    kind: Option<ObjectKind>,

    /// Address of the node from which to query the objects.
    ///
    /// NOTE: Note all object kinds will be supported in this mode.
    node_addr: Option<String>,
}

#[derive(Args)]
enum ObjectKind {
    #[arg(name = "jobs")]
    Job,

    #[arg(name = "workers")]
    Worker,

    #[arg(name = "blobs")]
    Blob,

    #[arg(name = "nodes")]
    Node,
}

#[derive(Args)]
pub struct StartJobCommand {
    #[arg(positional)]
    job_spec_path: String,
}

#[derive(Args)]
struct StartWorkerCommand {
    #[arg(positional)]
    worker_spec_path: String,

    /// Should be of the 'http(s)://ip:port'
    node_addr: String,
}

#[derive(Args)]
struct EventsCommand {
    worker_selector: WorkerNodeSelector,
}

#[derive(Args)]
struct LogCommand {
    worker_selector: WorkerNodeSelector,

    /// Id of the attempt from which to look up logs. If not specified, we will
    /// retrieve the logs of the currently running task attempt.
    attempt_id: Option<u64>,

    /// If true, we will look up the previous attempt (or the currently running
    /// one).
    latest_attempt: Option<bool>,
}

#[derive(Args)]
struct WorkerNodeSelector {
    /// Name of the worker from which to
    worker_name: String,

    node_addr: Option<String>,

    node_id: Option<u64>,
    /* TODO: Provide the attempt_id here as it may influence us to use a differnet node (one
     * that was previously assigned the worker)
     * - Given the attempt_id as a timestamp, we can search the WorkerMetadata in the metastore
     *   for the version of that record that was active at the time of the attempt (but need to
     *   be careful about checking ACLs for logs in this case) */
}

impl WorkerNodeSelector {
    async fn connect(&self) -> Result<NodeStubs> {
        let node_addr = match &self.node_addr {
            Some(addr) => addr.clone(),
            None => {
                // Must connect to the metastore, find the worker, and then we can

                let meta_client = Arc::new(ClusterMetaClient::create_from_environment().await?);

                let worker_meta = meta_client
                    .cluster_table::<WorkerMetadata>()
                    .get(&self.worker_name)
                    .await?
                    .ok_or_else(|| format_err!("No worker named: {}", self.worker_name))?;

                // TODO: assigned_node may eventually be allowed to be zero.
                let node_meta = meta_client
                    .cluster_table::<NodeMetadata>()
                    .get(&worker_meta.assigned_node())
                    .await?
                    .ok_or_else(|| err_msg("Failed to find node for worker"))?;

                node_meta.address().to_string()
            }
        };

        connect_to_node(&node_addr).await
    }
}

struct NodeStubs {
    service: ContainerNodeStub,
    blobs: BlobStoreStub,
}

async fn connect_to_node(node_addr: &str) -> Result<NodeStubs> {
    let channel = Arc::new(rpc::Http2Channel::create(node_addr).await?);

    Ok(NodeStubs {
        service: ContainerNodeStub::new(channel.clone()),
        blobs: BlobStoreStub::new(channel.clone()),
    })
}

/// Assuming that we have a single
///
/// TODO: Improve this so that we can continue running it if a previous run
/// failed.
async fn run_bootstrap(cmd: BootstrapCommand) -> Result<()> {
    let node = connect_to_node(&cmd.node_addr).await?;

    let request_context = rpc::ClientRequestContext::default();

    let node_meta = node
        .service
        .Identity(
            &request_context,
            &protobuf_builtins::google::protobuf::Empty::default(),
        )
        .await
        .result?;
    let node_id = node_meta.id();

    println!(
        "Bootstrapping cluster with node {}",
        base_radix::base32_encode_cl64(node_id)
    );
    println!("Zone: {}", node_meta.zone());

    // TODO: Much simpler to just have a top-level one
    let mut task_bundle = executor::bundle::TaskResultBundle::new();

    let zone = node_meta.zone().to_string();

    let metastore_resource = run_local_metastore(cmd.local_metastore_port, zone).await?;

    task_bundle.add(
        "Bootstrap",
        run_bootstrap_inner(
            node,
            request_context,
            node_meta,
            node_id,
            metastore_resource,
        ),
    );

    task_bundle.join().await
}

async fn run_local_metastore(port: u16, zone: String) -> Result<Arc<dyn ServiceResource>> {
    // TODO: Implement completely in memory.
    let local_metastore_dir = file::temp::TempDir::create()?;

    // TODO: Debuplicate with the job creation code.
    let mut route_label = raft::proto::RouteLabel::default();
    route_label.set_value(format!(
        "{}={}",
        cluster_client::meta::constants::ZONE_ENV_VAR,
        zone
    ));

    datastore::meta::store::run(datastore::meta::store::MetastoreOptions {
        dir: local_metastore_dir.path().to_owned(),
        init_port: 0,
        bootstrap: true,
        service_port: port,
        route_labels: vec![route_label],
        log: SegmentedLogOptions::default(),
        state_machine: EmbeddedDBStateMachineOptions::default(),
    })
    .await
}

async fn run_bootstrap_inner(
    node: NodeStubs,
    request_context: ClientRequestContext,
    node_meta: NodeMetadata,
    node_id: u64,
    local_metastore_resource: Arc<dyn ServiceResource>,
) -> Result<()> {
    // TODO: Given that we know the port of the local metastore, we can use that to
    // help find it.
    let meta_client = Arc::new(ClusterMetaClient::create(node_meta.zone()).await?);

    // Id of the local metastore server which is being used just for bootstrapping
    // the server.
    let local_server_id = {
        let status = meta_client.inner().current_status().await?;
        if status.configuration().servers_len() != 1 {
            return Err(err_msg("Expected exactly one metastore replica initially"));
        }

        // TODO: Find a better way of ensuring that this is definately the server that
        // is running in the local worker.
        let server = status.configuration().servers().iter().next().unwrap();
        if server.role() != Configuration_ServerRole::MEMBER {
            return Err(err_msg("First raft server is not a member"));
        }

        server.id()
    };

    // This is required so that the manager can schedule the metastore worker
    // immediately without retrying.
    println!("Waiting for node to register itself:");
    loop {
        if let Some(_) = meta_client
            .cluster_table::<NodeMetadata>()
            .get(&node_id)
            .await?
        {
            break;
        }

        executor::sleep(std::time::Duration::from_secs(1)).await;
    }
    println!("=> Done!");

    let mut zone_record = ZoneMetadata::default();
    zone_record.set_name(node_meta.zone());
    meta_client
        .cluster_table::<ZoneMetadata>()
        .put(&zone_record)
        .await?;

    let manager =
        Manager::new(meta_client.clone(), Arc::new(crypto::random::global_rng())).into_service();
    let manager_channel = Arc::new(rpc::LocalChannel::new(manager));
    let manager_stub = cluster_client::ManagerStub::new(manager_channel);

    // TODO: Verify that this actually has created the worker
    println!("Starting metastore job");
    let mut meta_job_spec = get_metastore_job(node_meta.zone()).await?;
    start_job_impl(
        meta_client.clone(),
        &manager_stub,
        &meta_job_spec,
        &request_context,
    )
    .await?;

    // Wait for the metastore to become part of the group.
    println!("Waiting for metastore replica to join group");
    loop {
        let status = meta_client.inner().current_status().await?;

        let mut done = false;
        for server in status.configuration().servers() {
            if server.id() == local_server_id {
                continue;
            }

            println!(
                "Found server {} with role {:?}",
                server.id().value(),
                server.role()
            );
            if server.role() == Configuration_ServerRole::MEMBER {
                done = true;
                break;
            }
        }

        if done {
            break;
        }

        executor::sleep(Duration::from_secs(4)).await;
    }
    println!("=> Done");

    println!("Removing local metastore replica");
    meta_client.inner().remove_server(local_server_id).await?;
    {
        local_metastore_resource
            .add_cancellation_token(Arc::new(AlreadyCancelledToken::default()))
            .await;
        local_metastore_resource.wait_for_termination().await?;
        drop(local_metastore_resource);
    }

    loop {
        // Wait for the local server to no longer be the leader.
        let status = match meta_client.inner().current_status().await {
            Ok(v) => v,
            Err(e) => {
                /*
                This may have one of two errors:
                - Failing because we tried directly connecting to the local metastore
                - Failing indirectly because we connected to the second replica and it piped our request to the remote server.

                TODO: Eventually we want to ensure that all these errors are eliminated through graceful leader transition.
                */
                // if let Some(status) = e.downcast_ref::<rpc::Status>() {
                //     // Requests may fail if trying to contact the currently stopping server.
                //     if status.code() == rpc::StatusCode::Unavailable {
                //         executor::sleep(Duration::from_secs(4)).await;
                //         continue;
                //     }
                // }

                eprintln!("- Failure connecting to metastore: {}", e);
                executor::sleep(Duration::from_secs(4)).await?;
                continue;
            }
        };
        if status.id() == local_server_id
            || status
                .configuration()
                .servers()
                .iter()
                .find(|s| s.id() == local_server_id)
                .is_some()
        {
            executor::sleep(Duration::from_secs(4)).await?;
            continue;
        } else {
            break;
        }
    }
    println!("=> Done");

    let mut manager_job_spec = get_manager_job().await?;

    start_job_impl(
        meta_client,
        &manager_stub,
        &manager_job_spec,
        &request_context,
    )
    .await?;

    Ok(())
}

async fn run_upgrade(cmd: UpgradeCommand) -> Result<()> {
    let meta_client = Arc::new(ClusterMetaClient::create_from_environment().await?);
    let manager_stub = connect_to_manager(meta_client.clone()).await?;
    let request_context = rpc::ClientRequestContext::default();

    let meta_job_spec = get_metastore_job(meta_client.zone()).await?;
    start_job_impl(
        meta_client.clone(),
        &manager_stub,
        &meta_job_spec,
        &request_context,
    )
    .await?;

    let manager_job_spec = get_manager_job().await?;
    start_job_impl(
        meta_client.clone(),
        &manager_stub,
        &manager_job_spec,
        &request_context,
    )
    .await?;

    Ok(())
}

async fn get_metastore_job(zone: &str) -> Result<JobSpec> {
    let mut meta_job_spec = JobSpec::parse_text(
        &file::read_to_string(project_path!("pkg/container/config/metastore.job")).await?,
    )?;

    meta_job_spec.worker_mut().add_args(format!(
        "--labels={}={}",
        cluster_client::meta::constants::ZONE_ENV_VAR,
        zone
    ));

    Ok(meta_job_spec)
}

async fn get_manager_job() -> Result<JobSpec> {
    JobSpec::parse_text(
        &file::read_to_string(project_path!("pkg/container/config/manager.job")).await?,
    )
}

async fn run_list(cmd: ListCommand) -> Result<()> {
    if let Some(node_addr) = &cmd.node_addr {
        let node = connect_to_node(node_addr).await?;

        let request_context = rpc::ClientRequestContext::default();

        let identity = node
            .service
            .Identity(
                &request_context,
                &protobuf_builtins::google::protobuf::Empty::default(),
            )
            .await
            .result?;

        println!("Nodes:");
        println!("{:?}", identity);

        println!("Workers:");
        let workers = node
            .service
            .ListWorkers(&request_context, &ListWorkersRequest::default())
            .await
            .result?;
        for worker in workers.workers() {
            println!("{:?}", worker);
        }

        println!("Blobs:");
        let blobs = node
            .blobs
            .List(
                &request_context,
                &protobuf_builtins::google::protobuf::Empty::default(),
            )
            .await
            .result?;
        for blob in blobs.blob() {
            println!("{:?}", blob);
        }

        return Ok(());
    }

    let meta_client = ClusterMetaClient::create_from_environment().await?;

    let kind = cmd.kind.unwrap();

    match kind {
        ObjectKind::Node => {
            println!("Nodes:");
            let nodes = meta_client.cluster_table::<NodeMetadata>().list().await?;
            for node in nodes {
                println!("{:?}", node);
            }
        }
        ObjectKind::Job => {
            println!("Jobs:");
            let jobs = meta_client.cluster_table::<JobMetadata>().list().await?;
            for job in jobs {
                println!("{:?}", job);
            }
        }
        ObjectKind::Worker => {
            let mut node_workers = HashMap::new();
            {
                let request_context = rpc::ClientRequestContext::default();
                let nodes = meta_client.cluster_table::<NodeMetadata>().list().await?;
                for node in nodes {
                    let node_stubs = connect_to_node(node.address()).await?;
                    let res = node_stubs
                        .service
                        .ListWorkers(&request_context, &ListWorkersRequest::default())
                        .await
                        .result?;

                    for worker in res.workers() {
                        node_workers.insert(worker.spec().name().to_string(), worker.clone());
                    }
                }
            }

            println!("Workers:");
            let workers = meta_client.cluster_table::<WorkerMetadata>().list().await?;

            let worker_states = meta_client
                .cluster_table::<WorkerStateMetadata>()
                .list()
                .await?
                .into_iter()
                .map(|s| (s.worker_name().to_string(), s))
                .collect::<HashMap<_, _>>();

            for worker in workers {
                let worker_state = worker_states
                    .get(worker.spec().name())
                    .cloned()
                    .unwrap_or_default();

                let mut node_state = String::new();
                if let Some(node_worker) = node_workers.get(worker.spec().name()) {
                    node_state = format!("\t({:?})", node_worker.state());
                }

                let state = {
                    if worker.drain() {
                        WorkerStateMetadata_ReportedState::DRAINING
                    } else if worker.revision() != worker_state.worker_revision() {
                        WorkerStateMetadata_ReportedState::UPDATING
                    } else {
                        worker_state.state()
                    }
                };

                println!("{}\t{:?}{}", worker.spec().name(), state, node_state);
            }
        }
        ObjectKind::Blob => {
            println!("Blobs:");
            let nodes = meta_client.cluster_table::<BlobMetadata>().list().await?;
            for node in nodes {
                println!("{:?}", node);
            }
        }
    }

    // let nodes = meta_client.list(b"/").await?;
    // for kv in nodes {
    //     println!("{:?}", kv);
    // }

    // meta_client.put(b"/hello", b"world").await?;

    // let value = meta_client.get(b"/cluster/node/c827b189377a40b4").await?;

    // println!("Got result: {:?}", common::bytes::Bytes::from(value));

    Ok(())
}

async fn run_start_worker(cmd: StartWorkerCommand) -> Result<()> {
    let node = connect_to_node(&cmd.node_addr).await?;

    let mut terminal_mode = false;

    let request_context = rpc::ClientRequestContext::default();

    let mut worker_spec = WorkerSpec::default();
    {
        let data = file::read_to_string(&cmd.worker_spec_path).await?;
        protobuf::text::parse_text_proto(&data, &mut worker_spec)
            .with_context(|e| format!("While reading {}: {}", cmd.worker_spec_path, e))?;
    }

    start_worker_impl(&node, &mut worker_spec, None, &request_context).await?;

    // TODO: Now wait for the worker to enter the Running state.
    // ^ this is required to ensure that we don't fetch logs for a past iteration of
    // the worker.

    // println!("Container Id: {}", start_response.container_id());

    // Currently this is a hack to ensure that any previous iteration of this worker
    // is stopped before we try getting the new logs.
    //
    // Instead we should look up the worker
    executor::sleep(std::time::Duration::from_secs(1)).await;

    let mut log_request = cluster_client::LogRequest::default();
    log_request.set_worker_name(worker_spec.name());

    // TODO: Deduplicate with the log command code.

    let mut log_stream = node.service.GetLogs(&request_context, &log_request).await;

    if terminal_mode {
        let stdin_task = start_terminal_input_task(
            &node.service,
            &request_context,
            worker_spec.name().to_string(),
        )
        .await?;
    }

    // TODO: Currently this seems to never unblock once the connection has been
    // closed.

    let mut stdout = file::Stdout::get();
    while let Some(entry) = log_stream.recv().await {
        // TODO: If we are not in terminal mode, restrict ourselves to only writing out
        // characters that are in the ASCII visible range (so that we can't
        // effect the terminal with escape codes).

        stdout.write_all(entry.value()).await?;
        stdout.flush().await?;
    }

    log_stream.finish().await?;

    if terminal_mode {
        // Always write the terminal reset sequence at the end.
        // TODO: Should should only be needed in
        // TODO: Ensure that this is always written even if the above code fails.
        stdout.write_all(&[0x1b, b'c']).await?;
        stdout.flush().await?;
    }

    Ok(())
}

async fn run_start_job(cmd: StartJobCommand) -> Result<()> {
    let meta_client = Arc::new(ClusterMetaClient::create_from_environment().await?);

    let manager_stub = connect_to_manager(meta_client.clone()).await?;

    let job_spec = JobSpec::parse_text(&file::read_to_string(cmd.job_spec_path).await?)?;

    let request_context = rpc::ClientRequestContext::default();

    start_job_impl(meta_client, &manager_stub, &job_spec, &request_context).await
}

async fn connect_to_manager(meta_client: Arc<ClusterMetaClient>) -> Result<ManagerStub> {
    let manager_channel = cluster_client::service::create_rpc_channel(
        "manager.system.job.local.cluster.internal",
        meta_client,
    )
    .await?;

    let manager_stub = ManagerStub::new(manager_channel);

    Ok(manager_stub)
}

async fn start_job_impl(
    meta_client: Arc<ClusterMetaClient>,
    manager: &ManagerStub,
    job_spec: &JobSpec,
    request_context: &rpc::ClientRequestContext,
) -> Result<()> {
    let mut job_spec = job_spec.clone();
    let mut blobs = build_worker_blobs(job_spec.worker_mut()).await?;

    let blob_allocations = {
        let mut req = AllocateBlobsRequest::default();
        for blob in &blobs {
            req.add_blob_specs(blob.spec().clone());
        }

        manager.AllocateBlobs(request_context, &req).await.result?
    };

    let blobs_by_id = blobs
        .into_iter()
        .map(|b| (b.spec().id().to_string(), b))
        .collect::<HashMap<_, _>>();

    // Upload blbos to all desired replicas.
    // TODO: Parallelize this
    for assignment in blob_allocations.new_assignments() {
        println!("Uploading: {:?}", assignment);

        let node = {
            let resolver = Arc::new(
                cluster_client::ServiceResolver::create(
                    &format!(
                        "{}.node.local.cluster.internal",
                        base_radix::base32_encode_cl64(assignment.node_id())
                    ),
                    meta_client.clone(),
                )
                .await?,
            );

            let channel = Arc::new(
                rpc::Http2Channel::create(http::ClientOptions::from_resolver(resolver)).await?,
            );

            NodeStubs {
                service: ContainerNodeStub::new(channel.clone()),
                blobs: BlobStoreStub::new(channel.clone()),
            }
        };

        let blob_data = blobs_by_id
            .get(assignment.blob_id())
            .ok_or_else(|| err_msg("Missing blob"))?;

        // TODO: Make sure this request fails fast if the node doesn't currently exist
        let mut request = node.blobs.Upload(request_context).await;
        request.send(&blob_data).await;

        if let Err(e) = request.finish().await {
            let mut ignore_error = false;
            if let Some(status) = e.downcast_ref::<rpc::Status>() {
                if status.code() == rpc::StatusCode::AlreadyExists {
                    println!("=> {}", status.message());
                    ignore_error = true;
                }
            }

            if !ignore_error {
                return Err(e);
            }
        }

        println!("Uploaded!");
    }

    let mut req = StartJobRequest::default();
    req.set_spec(job_spec);
    manager.StartJob(request_context, &req).await.result?;

    Ok(())
}

/// Directly starts a worker by contacting a node.
async fn start_worker_impl(
    node: &NodeStubs,
    worker_spec: &mut WorkerSpec,
    worker_revision: Option<u64>,
    request_context: &rpc::ClientRequestContext,
) -> Result<()> {
    // Look up all existing blobs on the node so that we can skip uploading them.
    let mut existing_blobs = HashSet::<String>::new();
    {
        let res = node
            .blobs
            .List(
                request_context,
                &protobuf_builtins::google::protobuf::Empty::default(),
            )
            .await
            .result?;
        for blob in res.blob() {
            existing_blobs.insert(blob.id().to_string());
        }
    }

    for blob_data in build_worker_blobs(worker_spec).await? {
        println!("=> Upload Blob: {}", blob_data.spec().id());
        if existing_blobs.contains(blob_data.spec().id()) {
            println!("Already uploaded");
            continue;
        }

        let mut request = node.blobs.Upload(request_context).await;
        request.send(&blob_data).await;

        if let Err(e) = request.finish().await {
            let mut ignore_error = false;
            if let Some(status) = e.downcast_ref::<rpc::Status>() {
                if status.code() == rpc::StatusCode::AlreadyExists {
                    println!("=> {}", status.message());
                    ignore_error = true;
                }
            }

            if !ignore_error {
                return Err(e);
            }
        }

        println!("Uploaded!");
    }

    // TODO: Interactive exec style runs should be interactive in the sense that
    // when the client's connection is closed, the container should also be
    // killed.

    println!("Starting server");

    let mut start_request = cluster_client::StartWorkerRequest::default();
    start_request.set_spec(worker_spec.clone());
    if let Some(rev) = worker_revision {
        start_request.set_revision(rev);
    }

    // start_request.worker_spec_mut().set_name("shell");
    // start_request.worker_spec_mut().add_args("/bin/bash".into());
    // start_request.worker_spec_mut().add_env("TERM=xterm-256color".into());

    let start_response = node
        .service
        .StartWorker(request_context, &start_request)
        .await
        .result?;

    Ok(())
}

async fn build_worker_blobs(worker_spec: &mut WorkerSpec) -> Result<Vec<cluster_client::BlobData>> {
    let mut out = vec![];

    let build_context = builder::BuildConfigTarget::default_for_local_machine()?;
    let mut builder_inst = builder::Builder::default()?;

    for volume in worker_spec.volumes_mut() {
        if let cluster_client::WorkerSpec_VolumeSourceCase::BuildTarget(label) =
            volume.source_case()
        {
            println!("Building volume target: {}", label);

            let res = builder_inst
                .build_target_cwd(label, builder::NATIVE_CONFIG_LABEL)
                .await?;

            // TODO: Instead just have the bundle_dir added to ouptut_files
            let (bundle_dir, bundle_spec) = {
                let (_, output_file) = res
                    .outputs
                    .output_files
                    .into_iter()
                    .find(|(r, _)| r.ends_with("/spec.textproto"))
                    .ok_or_else(|| err_msg("Failed to find bundle descriptor"))?;

                let text = file::read_to_string(&output_file.location).await?;
                let spec = BundleSpec::parse_text(&text)?;
                let dir = output_file.location.parent().unwrap().to_owned();

                (dir, spec)
            };

            volume.set_bundle(bundle_spec.clone());

            for variant in bundle_spec.variants() {
                let mut blob_data = cluster_client::BlobData::default();
                blob_data.set_spec(variant.blob().clone());

                let data = file::read(bundle_dir.join(variant.blob().id())).await?;
                blob_data.set_data(data);

                out.push(blob_data);
            }
        }
    }

    Ok(out)
}

async fn start_terminal_input_task(
    stub: &ContainerNodeStub,
    request_context: &ClientRequestContext,
    worker_name: String,
) -> Result<JoinHandle<()>> {
    let mut input_req = stub.WriteInput(&request_context).await;

    if !isatty(0)? {
        return Err(err_msg("Expected stdin to be a tty"));
    }

    // A good explanation of these flags is present in:
    // https://viewsourcecode.org/snaptoken/kilo/02.enteringRawMode.html#disable-raw-mode-at-exit

    let mut termios = tcgetattr(0)?;
    // Disable echoing of every input character to the output.
    termios.local_flags.remove(LocalFlags::ECHO);
    // Disable canonical mode: meaning we'll read bytes at a time instead of only
    // reading once an entire line was written.
    termios.local_flags.remove(LocalFlags::ICANON);
    // Disable receiving a signal for Ctrl-C and Ctrl-Z.
    // termios.local_flags.remove(LocalFlags::ISIG);
    // Disable Ctrl-S and Ctrl-Q.
    termios.input_flags.remove(InputFlags::IXON);
    // Disable Ctrl-V.
    termios.local_flags.remove(LocalFlags::IEXTEN);

    termios.input_flags.remove(InputFlags::ICRNL);
    termios.output_flags.remove(OutputFlags::OPOST);

    termios
        .input_flags
        .remove(InputFlags::BRKINT | InputFlags::INPCK | InputFlags::ISTRIP);
    termios.control_flags |= ControlFlags::CS8;

    tcsetattr(0, nix::sys::termios::SetArg::TCSAFLUSH, &termios)?;

    // TODO: When we create the tty on the server, do we need to explicitly enable
    // all of the above flags.

    Ok(executor::spawn(async move {
        let mut stdin = file::Stdin::get();

        loop {
            let mut data = [0u8; 512];

            let n = stdin.read(&mut data).await.expect("Stdin Read failed");
            if n == 0 {
                println!("EOI");
                break;
            }

            let mut input = WriteInputRequest::default();
            input.set_worker_name(&worker_name);
            input.set_data(data[0..n].to_vec());

            if !input_req.send(&input).await {
                break;
            }
        }

        let res = input_req.finish().await;
        println!("{:?}", res);
    }))
}

async fn run_log(cmd: LogCommand) -> Result<()> {
    let node = cmd.worker_selector.connect().await?;

    let request_context = rpc::ClientRequestContext::default();

    let mut log_request = cluster_client::LogRequest::default();
    log_request.set_worker_name(&cmd.worker_selector.worker_name);

    if let Some(num) = cmd.attempt_id {
        log_request.set_attempt_id(num);
    }

    if cmd.latest_attempt == Some(true) {
        let mut request = cluster_client::GetEventsRequest::default();
        request.set_worker_name(&cmd.worker_selector.worker_name);

        let mut resp = node
            .service
            .GetEvents(&request_context, &request)
            .await
            .result?;

        for event in resp.events() {
            if event.has_started() && event.timestamp() > log_request.attempt_id() {
                log_request.set_attempt_id(event.timestamp());
            }
        }
    }

    let mut log_stream = node.service.GetLogs(&request_context, &log_request).await;

    while let Some(entry) = log_stream.recv().await {
        let value = std::str::from_utf8(entry.value())?;
        print!("{}", value);
        // common::async_std::io::stdout().flush().await?;
    }

    log_stream.finish().await?;

    println!("<End of log>");

    Ok(())
}

async fn run_events(cmd: EventsCommand) -> Result<()> {
    let node = cmd.worker_selector.connect().await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut request = cluster_client::GetEventsRequest::default();
    request.set_worker_name(&cmd.worker_selector.worker_name);

    let mut resp = node
        .service
        .GetEvents(&request_context, &request)
        .await
        .result?;

    struct Attempt<'a> {
        id: u64,
        start_time: SystemTime,
        end_time: Option<SystemTime>,
        exit_status: Option<cluster_client::ContainerStatus>,
        events: Vec<&'a cluster_client::WorkerEvent>,
    }

    resp.events_mut()
        .sort_by(|a, b| a.timestamp().cmp(&b.timestamp()));

    let mut attempts = vec![];

    // TODO: If the final attempt (or any event) doesn't have a Stopped event, it
    // may still not be running if the event failed to be saved. Need to cross
    // reference with the current state of the worker on the node.
    for event in resp.events() {
        let time = std::time::UNIX_EPOCH + Duration::from_micros(event.timestamp());

        // TODO: Will eventually need to handle StartFailure
        match event.typ_case() {
            cluster_client::WorkerEventTypeCase::Started(_) => attempts.push(Attempt {
                id: event.timestamp(),
                start_time: time,
                end_time: None,
                exit_status: None,
                events: vec![],
            }),
            cluster_client::WorkerEventTypeCase::StartFailure(v) => attempts.push(Attempt {
                id: event.timestamp(),
                start_time: time,
                end_time: Some(time.clone()),
                exit_status: None,
                events: vec![],
            }),
            cluster_client::WorkerEventTypeCase::Stopped(e) => {
                let last_attempt = attempts.last_mut().unwrap();
                last_attempt.exit_status = Some(e.status().clone());
                last_attempt.end_time = Some(time);
            }
            _ => {}
        }

        let last_attempt = attempts.last_mut().unwrap();
        last_attempt.events.push(&event);

        // println!("{:?}", event);
    }

    for attempt in attempts {
        println!("{}: {}", attempt.id, time_to_string(&attempt.start_time));
        if let Some(end_time) = attempt.end_time {
            println!("=> {:?}", attempt.exit_status.unwrap());
        }
    }

    Ok(())
}

fn time_to_string(time: &SystemTime) -> String {
    common::chrono::DateTime::<common::chrono::Local>::from(*time).to_rfc2822()
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    match args.command {
        Command::Bootstrap(cmd) => run_bootstrap(cmd).await,
        Command::Upgrade(cmd) => run_upgrade(cmd).await,
        Command::List(cmd) => run_list(cmd).await,
        Command::StartWorker(cmd) => run_start_worker(cmd).await,
        Command::Log(cmd) => run_log(cmd).await,
        Command::StartJob(cmd) => run_start_job(cmd).await,
        Command::Events(cmd) => run_events(cmd).await,
    }
}
