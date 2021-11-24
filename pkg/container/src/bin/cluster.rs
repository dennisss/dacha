// TODO: Combine all of the CLI utilities into this one.

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

use async_std::task::JoinHandle;
use builder::proto::bundle::{BlobFormat, BundleSpec};
use common::async_std::fs;
use common::async_std::io::ReadExt;
use common::async_std::task;
use common::errors::*;
use common::failure::ResultExt;
use common::futures::AsyncWriteExt;
use common::task::ChildTask;
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use datastore::meta::client::{MetastoreClient, MetastoreClientInterface};
use nix::{
    sys::termios::{tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags},
    unistd::isatty,
};
use protobuf::text::parse_text_proto;
use protobuf::text::ParseTextProto;
use protobuf::Message;
use rpc::ClientRequestContext;

use container::{
    meta::*, AllocateBlobsRequest, AllocateBlobsResponse, BlobMetadata, JobSpec, ManagerStub,
    NodeMetadata, StartJobRequest,
};
use container::{
    BlobStoreStub, ContainerNodeStub, JobMetadata, NodeMetadata_State, TaskMetadata,
    TaskMetadata_State, TaskSpec, TaskSpec_Port, TaskSpec_Volume, WriteInputRequest, ZoneMetadata,
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

    #[arg(name = "list")]
    List(ListCommand),

    #[arg(name = "start_job")]
    StartJob(StartJobCommand),

    /// Start a single task directly on a node. This is mainly for cluster
    /// bootstrapping.
    #[arg(name = "start_task")]
    StartTask(StartTaskCommand),

    #[arg(name = "logs")]
    Logs(LogsCommand),
}

#[derive(Args)]
struct BootstrapCommand {
    node: String,
}

#[derive(Args)]
struct ListCommand {}

#[derive(Args)]
pub struct StartJobCommand {
    #[arg(positional)]
    job_spec_path: String,
}

#[derive(Args)]
struct StartTaskCommand {
    #[arg(positional)]
    task_spec_path: String,

    node: String,
}

#[derive(Args)]
struct LogsCommand {
    task_name: String,
    node: String,
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
    let node = connect_to_node(&cmd.node).await?;

    let request_context = rpc::ClientRequestContext::default();

    let node_meta = node
        .service
        .Identity(&request_context, &google::proto::empty::Empty::default())
        .await
        .result?;
    let node_id = node_meta.id();

    println!("Bootstrapping cluster with node {:08x}", node_id);

    // TODO: Need to build any build volumes to blobs.
    let mut meta_task_spec = TaskSpec::parse_text(
        &fs::read_to_string(project_path!("pkg/datastore/config/metastore.task")).await?,
    )?;

    // TODO: Assign a port to the spec.

    // Will build the binary bundle and start the single task on the node.
    start_task_impl(&node, &mut meta_task_spec, &request_context).await?;

    // TODO: Call bootstrap on the metastore instance (main challenge is to ensure
    // that only one metastore group exists if we need to retry bootstrapping).

    {
        let mut node_uri = http::uri::Uri::from_str(&format!("http://{}", cmd.node))?;
        node_uri.authority.as_mut().unwrap().port = Some(30001);

        let bootstrap_client = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
            &node_uri,
        )?)?);

        let stub = raft::ServerInitStub::new(bootstrap_client);

        // TODO: Ignore method not found errors (which would imply that we are already
        // bootstrapped).
        stub.Bootstrap(&request_context, &raft::BootstrapRequest::default())
            .await
            .result?;
    }

    let meta_client = Arc::new(MetastoreClient::create().await?);

    // Wait for the node to register itself

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
    zone_record.set_name("home");
    meta_client
        .cluster_table::<ZoneMetadata>()
        .put(&zone_record)
        .await?;

    let mut meta_job_record = JobMetadata::default();
    meta_job_record.spec_mut().set_name("system.meta");
    meta_job_record.spec_mut().set_replicas(1u32);
    meta_job_record.spec_mut().set_task(meta_task_spec.clone());
    meta_job_record.set_task_revision(0u64);

    meta_client
        .cluster_table::<JobMetadata>()
        .put(&meta_job_record)
        .await?;

    let mut meta_task_record = TaskMetadata::default();
    meta_task_record.set_spec(meta_task_spec.clone());
    meta_task_record.set_assigned_node(node_id);
    meta_task_record.set_revision(0u64);
    meta_task_record.set_state(TaskMetadata_State::STARTING);
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

        for port in meta_task_spec.ports() {
            node_meta.allocated_ports_mut().insert(port.number());
        }

        txn.cluster_table::<NodeMetadata>().put(&node_meta).await?;

        txn.commit().await?;
    }

    // TOOD: Now bring up a local manager instance and use it to start a manager job
    // on the cluster.

    let manager_thread = ChildTask::spawn(run_manager());

    let mut manager_job_spec = JobSpec::parse_text(
        &fs::read_to_string(project_path!("pkg/container/config/manager.job")).await?,
    )?;

    let manager_channel = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
        &format!("http://127.0.0.1:{}", 10500).parse()?,
    )?)?);

    let manager_stub = container::ManagerStub::new(manager_channel);

    start_job_impl(
        meta_client,
        &manager_stub,
        &manager_job_spec,
        &request_context,
    )
    .await?;

    drop(manager_thread);

    Ok(())
}

async fn run_manager() {
    if let Err(e) = container::manager_main_with_port(10500).await {
        eprintln!("Manager failed: {}", e);
    }
}

async fn run_list(cmd: ListCommand) -> Result<()> {
    let meta_client = datastore::meta::client::MetastoreClient::create().await?;

    println!("Nodes:");
    let nodes = meta_client.cluster_table::<NodeMetadata>().list().await?;
    for node in nodes {
        println!("{:?}", node);
    }

    println!("Jobs:");
    let nodes = meta_client.cluster_table::<JobMetadata>().list().await?;
    for node in nodes {
        println!("{:?}", node);
    }

    println!("Tasks:");
    let nodes = meta_client.cluster_table::<TaskMetadata>().list().await?;
    for node in nodes {
        println!("{:?}", node);
    }

    println!("Blobs:");
    let nodes = meta_client.cluster_table::<BlobMetadata>().list().await?;
    for node in nodes {
        println!("{:?}", node);
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
    let node = connect_to_node(&cmd.node).await?;

    let mut terminal_mode = false;

    let request_context = rpc::ClientRequestContext::default();

    let mut task_spec = TaskSpec::default();
    {
        let data = fs::read_to_string(&cmd.task_spec_path).await?;
        protobuf::text::parse_text_proto(&data, &mut task_spec)?;
    }

    start_task_impl(&node, &mut task_spec, &request_context).await?;

    // TODO: Now wait for the task to enter the Running state.
    // ^ this is required to ensure that we don't fetch logs for a past iteration of
    // the task.

    // println!("Container Id: {}", start_response.container_id());

    // Currently this is a hack to ensure that any previous iteration of this task
    // is stopped before we try getting the new logs.
    common::async_std::task::sleep(std::time::Duration::from_secs(1)).await;

    let mut log_request = container::LogRequest::default();
    log_request.set_task_name(task_spec.name());

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
    let meta_client = Arc::new(MetastoreClient::create().await?);

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
    meta_client: Arc<MetastoreClient>,
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

async fn run_logs(cmd: LogsCommand) -> Result<()> {
    let node = connect_to_node(&cmd.node).await?;
    let request_context = rpc::ClientRequestContext::default();

    let mut log_request = container::LogRequest::default();
    log_request.set_task_name(&cmd.task_name);

    let mut log_stream = node.service.GetLogs(&request_context, &log_request).await;

    while let Some(entry) = log_stream.recv().await {
        let value = std::str::from_utf8(entry.value())?;
        print!("{}", value);
        // common::async_std::io::stdout().flush().await?;
    }

    log_stream.finish().await?;

    Ok(())
}

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    match args.command {
        Command::Bootstrap(cmd) => run_bootstrap(cmd).await,
        Command::List(cmd) => run_list(cmd).await,
        Command::StartTask(cmd) => run_start_task(cmd).await,
        Command::Logs(cmd) => run_logs(cmd).await,
        Command::StartJob(cmd) => run_start_job(cmd).await,
    }
}

fn main() -> Result<()> {
    task::block_on(run())
}
