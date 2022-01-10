// Cluster management CLI
// TODO: Combine all of the CLI utilities into this one.
/*
Aside from the 'bootstrap' command, all commands require

Testing:
    cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto

    cargo run --bin cluster -- start_task --node_addr=127.0.0.1:10400 pkg/rpc_test/config/adder_server.task

Next steps:
-

Testing with a single node cluster:
    cargo run --bin cluster_node -- --config=pkg/container/config/node.textproto --zone=dev

    cargo run --bin cluster -- bootstrap --node_addr=127.0.0.1:10400

    CLUSTER_ZONE=dev cargo run --bin cluster -- list jobs

    CLUSTER_ZONE=dev cargo run --bin cluster -- start_job pkg/rpc_test/config/adder_server.job

    CLUSTER_ZONE=dev cargo run --bin adder_client -- --target=adder_server.job.local.cluster.internal

    CLUSTER_ZONE=dev cargo run --bin cluster -- log --task_name=adder_server.256326fbfc425883

    <try modifying the adder_server job and rerunning the start_job / adder_client code to verify that we can update to the new revision>

    <try stopping and restarting the node. everything should still work>


*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

// TODO: Given this is only used for bootstrapping, consider refactoring out
// this dependency.
extern crate raft;

use std::collections::HashMap;
use std::str::FromStr;
use std::{collections::HashSet, sync::Arc};

use builder::proto::bundle::{BlobFormat, BundleSpec};
use common::async_std::fs;
use common::async_std::io::ReadExt;
use common::async_std::task;
use common::async_std::task::JoinHandle;
use common::errors::*;
use common::failure::ResultExt;
use common::futures::AsyncWriteExt;
use common::task::ChildTask;
use container::manager::Manager;
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use crypto::sip::SipHasher;
use datastore::meta::client::MetastoreClientInterface;
use nix::{
    sys::termios::{tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags},
    unistd::isatty,
};
use protobuf::text::parse_text_proto;
use protobuf::text::ParseTextProto;
use protobuf::Message;
use rpc::ClientRequestContext;

use container::meta::client::ClusterMetaClient;
use container::{
    meta::*, AllocateBlobsRequest, AllocateBlobsResponse, BlobMetadata, JobSpec, ListTasksRequest,
    ManagerIntoService, ManagerStub, NodeMetadata, StartJobRequest,
};
use container::{
    BlobStoreStub, ContainerNodeStub, JobMetadata, NodeMetadata_State, TaskMetadata, TaskSpec,
    TaskSpec_Port, TaskSpec_Volume, TaskStateMetadata, WriteInputRequest, ZoneMetadata,
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

    /// Enumerate objects (tasks, )
    #[arg(name = "list")]
    List(ListCommand),

    #[arg(name = "start_job")]
    StartJob(StartJobCommand),

    /// Start a single task directly on a node. This is mainly for cluster
    /// bootstrapping.
    #[arg(name = "start_task")]
    StartTask(StartTaskCommand),

    #[arg(name = "events")]
    Events(EventsCommand),

    /// Retrieve the log (stdout/stderr) outputs of a task.
    #[arg(name = "log")]
    Log(LogCommand),
}

#[derive(Args)]
struct BootstrapCommand {
    node_addr: String,
}

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

    #[arg(name = "tasks")]
    Task,

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
struct StartTaskCommand {
    #[arg(positional)]
    task_spec_path: String,

    node_addr: String,
}

#[derive(Args)]
struct EventsCommand {
    task_selector: TaskNodeSelector,
}

#[derive(Args)]
struct LogCommand {
    task_selector: TaskNodeSelector,

    /// Id of the attempt from which to look up logs. If not specified, we will
    /// retrieve the logs of the currently running task attempt.
    attempt_id: Option<u64>,
}

#[derive(Args)]
struct TaskNodeSelector {
    /// Name of the task from which to
    task_name: String,

    node_addr: Option<String>,

    node_id: Option<u64>,
    /* TODO: Provide the attempt_id here as it may influence us to use a differnet node (one
     * that was previously assigned the task)
     * - Given the attempt_id as a timestamp, we can search the TaskMetadata in the metastore
     *   for the version of that record that was active at the time of the attempt (but need to
     *   be careful about checking ACLs for logs in this case) */
}

impl TaskNodeSelector {
    async fn connect(&self) -> Result<NodeStubs> {
        let node_addr = match &self.node_addr {
            Some(addr) => addr.clone(),
            None => {
                // Must connect to the metastore, find the task, and then we can

                let meta_client = Arc::new(ClusterMetaClient::create_from_environment().await?);

                let task_meta = meta_client
                    .cluster_table::<TaskMetadata>()
                    .get(&self.task_name)
                    .await?
                    .ok_or_else(|| format_err!("No task named: {}", self.task_name))?;

                // TODO: assigned_node may eventually be allowed to be zero.
                let node_meta = meta_client
                    .cluster_table::<NodeMetadata>()
                    .get(&task_meta.assigned_node())
                    .await?
                    .ok_or_else(|| err_msg("Failed to find node for task"))?;

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
    let channel = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
        &format!("http://{}", node_addr).parse()?,
    )?)?);

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
        .Identity(&request_context, &google::proto::empty::Empty::default())
        .await
        .result?;
    let node_id = node_meta.id();

    println!("Bootstrapping cluster with node {:08x}", node_id);
    println!("Zone: {}", node_meta.zone());

    // TODO: We also need to bootstrap the BlobMetadata that is used the metastore.

    // TODO: Need to build any build volumes to blobs.
    let mut meta_job_spec = JobSpec::parse_text(
        &fs::read_to_string(project_path!("pkg/container/config/metastore.job")).await?,
    )?;

    meta_job_spec.task_mut().add_args(format!(
        "--labels={}={}",
        container::meta::constants::ZONE_ENV_VAR,
        node_meta.zone()
    ));

    const METASTORE_INITIAL_PORT: usize = 30001;

    // NOTE: The assumption is that if the metastore job is later updated, it is
    // never assigned a new revision <= 1 by the manager.
    const METASTORE_INITIAL_REVISION: u64 = 1;

    // NOTE: This is kind of hacky as typically the JobSpec should not contain a
    // TaskSpec for ports assigned, or task names asssigned, but it's simpler to
    // assign these for the first metastore instance. This will all be cleaned up
    // later if the meta store is scaled up.
    //
    // TODO: Try to re-use the manager code for doing this stuff.
    meta_job_spec.task_mut().ports_mut()[0].set_number(METASTORE_INITIAL_PORT as u32);
    // While we usually generate a random id, the first replica will have a
    // deterministic id so that it's easier to retry the bootstrap command if it
    // partially fails.
    let meta_job_name = meta_job_spec.name().to_string();
    meta_job_spec
        .task_mut()
        .set_name(format!("{}.{:08x}", meta_job_name, {
            let mut hasher = SipHasher::default_rounds_with_key_halves(0, 0);
            hasher.update(node_meta.zone().as_bytes());
            hasher.finish_u64()
        }));

    // NOTE: This will re-write any build_target references in the
    // meta_job_spec.task_mut() to blob references so that we can store it in
    // the meta store later.
    start_task_impl(
        &node,
        meta_job_spec.task_mut(),
        Some(METASTORE_INITIAL_REVISION),
        &request_context,
    )
    .await?;

    {
        println!("Bootstrapping metastore");

        let mut node_uri = http::uri::Uri::from_str(&format!("http://{}", cmd.node_addr))?;
        node_uri.authority.as_mut().unwrap().port = Some(METASTORE_INITIAL_PORT as u16);

        let bootstrap_client = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
            &node_uri,
        )?)?);

        let stub = raft::ServerInitStub::new(bootstrap_client);

        // TODO: Ignore method not found errors (which would imply that we are already
        // bootstrapped).
        if let Err(e) = stub
            .Bootstrap(&request_context, &raft::BootstrapRequest::default())
            .await
            .result
        {
            if let Some(status) = e.downcast_ref::<rpc::Status>() {
                if status.code() == rpc::StatusCode::Unimplemented {
                    // Likely the method doesn't exist so the metastore is probably already
                    // bootstrapped.
                    println!("=> Already bootstrapped");
                } else {
                    return Err(e);
                }
            } else {
                return Err(e);
            }
        } else {
            println!("=> Done!");
        }
    }

    let meta_client = Arc::new(ClusterMetaClient::create(node_meta.zone()).await?);

    println!("Waiting for node to register itself:");
    loop {
        if let Some(_) = meta_client
            .cluster_table::<NodeMetadata>()
            .get(&node_id)
            .await?
        {
            break;
        }

        common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;
    }
    println!("=> Done!");

    // TODO: We should instead infer the zone from some flag on the node.
    let mut zone_record = ZoneMetadata::default();
    zone_record.set_name(node_meta.zone());
    meta_client
        .cluster_table::<ZoneMetadata>()
        .put(&zone_record)
        .await?;

    let mut meta_job_record = JobMetadata::default();
    meta_job_record.set_spec(meta_job_spec);
    meta_job_record.set_task_revision(METASTORE_INITIAL_REVISION);

    meta_client
        .cluster_table::<JobMetadata>()
        .put(&meta_job_record)
        .await?;

    let mut meta_task_record = TaskMetadata::default();
    meta_task_record.set_spec(meta_job_record.spec().task().clone());
    meta_task_record.set_assigned_node(node_id);
    meta_task_record.set_revision(meta_job_record.task_revision());
    meta_client
        .cluster_table::<TaskMetadata>()
        .put(&meta_task_record)
        .await?;

    // Mark the node as active.
    // This can't be done until the TaskMetadata is added to the metastore to
    // prevent the node deleting the unknown metastore task.
    {
        let txn = meta_client.new_transaction().await?;
        let mut node_meta = meta_client
            .cluster_table::<NodeMetadata>()
            .get(&node_id)
            .await?
            .ok_or_else(|| err_msg("Node metadata disappeared?"))?;

        node_meta.set_state(NodeMetadata_State::ACTIVE);

        for port in meta_task_record.spec().ports() {
            node_meta.allocated_ports_mut().insert(port.number());
        }

        txn.cluster_table::<NodeMetadata>().put(&node_meta).await?;

        txn.commit().await?;
    }

    // TOOD: Now bring up a local manager instance and use it to start a manager job
    // on the cluster.

    let manager = Manager::new(meta_client.clone()).into_service();
    let manager_channel = Arc::new(rpc::LocalChannel::new(manager));

    let mut manager_job_spec = JobSpec::parse_text(
        &fs::read_to_string(project_path!("pkg/container/config/manager.job")).await?,
    )?;

    let manager_stub = container::ManagerStub::new(manager_channel);

    start_job_impl(
        meta_client,
        &manager_stub,
        &manager_job_spec,
        &request_context,
    )
    .await?;

    Ok(())
}

async fn run_list(cmd: ListCommand) -> Result<()> {
    if let Some(node_addr) = &cmd.node_addr {
        let node = connect_to_node(node_addr).await?;

        let request_context = rpc::ClientRequestContext::default();

        let identity = node
            .service
            .Identity(&request_context, &google::proto::empty::Empty::default())
            .await
            .result?;

        println!("Nodes:");
        println!("{:?}", identity);

        println!("Tasks:");
        let tasks = node
            .service
            .ListTasks(&request_context, &ListTasksRequest::default())
            .await
            .result?;
        for task in tasks.tasks() {
            println!("{:?}", task);
        }

        println!("Blobs:");
        let blobs = node
            .blobs
            .List(&request_context, &google::proto::empty::Empty::default())
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
        ObjectKind::Task => {
            println!("Tasks:");
            let tasks = meta_client.cluster_table::<TaskMetadata>().list().await?;

            let task_states = meta_client
                .cluster_table::<TaskStateMetadata>()
                .list()
                .await?
                .into_iter()
                .map(|s| (s.task_name().to_string(), s))
                .collect::<HashMap<_, _>>();

            for task in tasks {
                let task_state = task_states
                    .get(task.spec().name())
                    .cloned()
                    .unwrap_or_default();

                println!("{}\t{:?}", task.spec().name(), task_state.state());

                // println!("{:?}", task);
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

async fn run_start_task(cmd: StartTaskCommand) -> Result<()> {
    let node = connect_to_node(&cmd.node_addr).await?;

    let mut terminal_mode = false;

    let request_context = rpc::ClientRequestContext::default();

    let mut task_spec = TaskSpec::default();
    {
        let data = fs::read_to_string(&cmd.task_spec_path).await?;
        protobuf::text::parse_text_proto(&data, &mut task_spec)
            .with_context(|e| format!("While reading {}: {}", cmd.task_spec_path, e))?;
    }

    start_task_impl(&node, &mut task_spec, None, &request_context).await?;

    // TODO: Now wait for the task to enter the Running state.
    // ^ this is required to ensure that we don't fetch logs for a past iteration of
    // the task.

    // println!("Container Id: {}", start_response.container_id());

    // Currently this is a hack to ensure that any previous iteration of this task
    // is stopped before we try getting the new logs.
    //
    // Instead we should look up the task
    common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;

    let mut log_request = container::LogRequest::default();
    log_request.set_task_name(task_spec.name());

    // TODO: Deduplicate with the log command code.

    let mut log_stream = node.service.GetLogs(&request_context, &log_request).await;

    if terminal_mode {
        let stdin_task = start_terminal_input_task(
            &node.service,
            &request_context,
            task_spec.name().to_string(),
        )
        .await?;
    }

    // TODO: Currently this seems to never unblock once the connection has been
    // closed.

    let mut stdout = common::async_std::io::stdout();
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

    let resolver = Arc::new(
        container::ServiceResolver::create(
            "manager.system.job.local.cluster.internal",
            meta_client.clone(),
        )
        .await?,
    );

    let manager_channel = rpc::Http2Channel::create(http::ClientOptions::from_resolver(resolver))?;

    let manager_stub = ManagerStub::new(Arc::new(manager_channel));

    let job_spec = JobSpec::parse_text(&fs::read_to_string(cmd.job_spec_path).await?)?;

    let request_context = rpc::ClientRequestContext::default();

    start_job_impl(meta_client, &manager_stub, &job_spec, &request_context).await
}

async fn start_job_impl(
    meta_client: Arc<ClusterMetaClient>,
    manager: &ManagerStub,
    job_spec: &JobSpec,
    request_context: &rpc::ClientRequestContext,
) -> Result<()> {
    let mut job_spec = job_spec.clone();
    let mut blobs = build_task_blobs(job_spec.task_mut()).await?;

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
                container::ServiceResolver::create(
                    &format!("{:08x}.node.local.cluster.internal", assignment.node_id()),
                    meta_client.clone(),
                )
                .await?,
            );

            let channel = Arc::new(rpc::Http2Channel::create(
                http::ClientOptions::from_resolver(resolver),
            )?);

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

/// Directly starts a task by contacting a node.
async fn start_task_impl(
    node: &NodeStubs,
    task_spec: &mut TaskSpec,
    task_revision: Option<u64>,
    request_context: &rpc::ClientRequestContext,
) -> Result<()> {
    // Look up all existing blobs on the node so that we can skip uploading them.
    let mut existing_blobs = HashSet::<String>::new();
    {
        let res = node
            .blobs
            .List(request_context, &google::proto::empty::Empty::default())
            .await
            .result?;
        for blob in res.blob() {
            existing_blobs.insert(blob.id().to_string());
        }
    }

    for blob_data in build_task_blobs(task_spec).await? {
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

    let mut start_request = container::StartTaskRequest::default();
    start_request.set_spec(task_spec.clone());
    if let Some(rev) = task_revision {
        start_request.set_revision(rev);
    }

    // start_request.task_spec_mut().set_name("shell");
    // start_request.task_spec_mut().add_args("/bin/bash".into());
    // start_request.task_spec_mut().add_env("TERM=xterm-256color".into());

    let start_response = node
        .service
        .StartTask(request_context, &start_request)
        .await
        .result?;

    Ok(())
}

async fn build_task_blobs(task_spec: &mut TaskSpec) -> Result<Vec<container::BlobData>> {
    let mut out = vec![];

    let build_context = builder::BuildContext::default_for_local_machine().await?;
    let mut builder_inst = builder::Builder::default();

    for volume in task_spec.volumes_mut() {
        if let container::TaskSpec_VolumeSourceCase::BuildTarget(label) = volume.source_case() {
            println!("Building volume target: {}", label);

            let res = builder_inst.build_target_cwd(label, &build_context).await?;

            // TODO: Instead just have the bundle_dir added to ouptut_files
            let (bundle_dir, bundle_spec) = {
                let (_, path) = res
                    .output_files
                    .into_iter()
                    .find(|(r, _)| r.ends_with("/spec.textproto"))
                    .ok_or_else(|| err_msg("Failed to find bundle descriptor"))?;

                let text = fs::read_to_string(&path).await?;
                let spec = BundleSpec::parse_text(&text)?;
                let dir = path.parent().unwrap().to_path_buf();

                (dir, spec)
            };

            volume.set_bundle(bundle_spec.clone());

            for variant in bundle_spec.variants() {
                let mut blob_data = container::BlobData::default();
                blob_data.set_spec(variant.blob().clone());

                let data = fs::read(bundle_dir.join(variant.blob().id())).await?;
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
    task_name: String,
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

    Ok(task::spawn(async move {
        let mut stdin = common::async_std::io::stdin();

        loop {
            let mut data = [0u8; 512];

            let n = stdin.read(&mut data).await.expect("Stdin Read failed");
            if n == 0 {
                println!("EOI");
                break;
            }

            let mut input = WriteInputRequest::default();
            input.set_task_name(&task_name);
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
    let node = cmd.task_selector.connect().await?;

    let request_context = rpc::ClientRequestContext::default();

    let mut log_request = container::LogRequest::default();
    log_request.set_task_name(&cmd.task_selector.task_name);

    if let Some(num) = cmd.attempt_id {
        log_request.set_attempt_id(num);
    }

    let mut log_stream = node.service.GetLogs(&request_context, &log_request).await;

    while let Some(entry) = log_stream.recv().await {
        let value = std::str::from_utf8(entry.value())?;
        print!("{}", value);
        // common::async_std::io::stdout().flush().await?;
    }

    log_stream.finish().await?;

    Ok(())
}

async fn run_events(cmd: EventsCommand) -> Result<()> {
    let node = cmd.task_selector.connect().await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut request = container::GetEventsRequest::default();
    request.set_task_name(&cmd.task_selector.task_name);

    let resp = node
        .service
        .GetEvents(&request_context, &request)
        .await
        .result?;
    for event in resp.events() {
        println!("{:?}", event);
    }

    Ok(())
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    match args.command {
        Command::Bootstrap(cmd) => run_bootstrap(cmd).await,
        Command::List(cmd) => run_list(cmd).await,
        Command::StartTask(cmd) => run_start_task(cmd).await,
        Command::Log(cmd) => run_log(cmd).await,
        Command::StartJob(cmd) => run_start_job(cmd).await,
        Command::Events(cmd) => run_events(cmd).await,
    }
}

fn main() -> Result<()> {
    task::block_on(run())
}
