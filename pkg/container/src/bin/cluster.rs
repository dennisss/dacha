// TODO: Combine all of the CLI utilities into this one.

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

// TODO: Given this is only used for bootstrapping, consider refactoring out
// this dependency.
extern crate raft;

use std::str::FromStr;
use std::{collections::HashSet, sync::Arc};

use async_std::task::JoinHandle;
use common::async_std::fs;
use common::async_std::io::ReadExt;
use common::async_std::task;
use common::errors::*;
use common::failure::ResultExt;
use common::futures::AsyncWriteExt;
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use nix::{
    sys::termios::{tcgetattr, tcsetattr, ControlFlags, InputFlags, LocalFlags, OutputFlags},
    unistd::isatty,
};
use protobuf::text::parse_text_proto;
use protobuf::text::ParseTextProto;
use protobuf::Message;
use rpc::ClientRequestContext;

use container::{
    BlobStoreStub, ContainerNodeStub, JobMetadata, TaskMetadata, TaskSpec, TaskSpec_Port,
    TaskSpec_Volume, WriteInputRequest, ZoneMetadata,
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

    let meta_client = datastore::meta::client::MetastoreClient::create().await?;

    // TODO: We should instead infer the zone from some flag on the node.
    let mut zone_record = ZoneMetadata::default();
    zone_record.set_name("home");
    meta_client
        .put(b"/cluster/zone", &zone_record.serialize()?)
        .await?;

    let mut meta_job_record = JobMetadata::default();
    meta_job_record.spec_mut().set_name("system.meta");
    meta_job_record.spec_mut().set_replicas(1u32);
    meta_job_record.spec_mut().set_task(meta_task_spec.clone());
    meta_client
        .put(
            format!("/cluster/job/{}", meta_job_record.spec().name()).as_bytes(),
            &meta_job_record.serialize()?,
        )
        .await?;

    let mut meta_task_record = TaskMetadata::default();
    meta_task_record.set_spec(meta_task_spec.clone());
    meta_task_record.set_assigned_node(node_id);
    meta_client
        .put(
            format!("/cluster/task/{}", meta_task_record.spec().name()).as_bytes(),
            &meta_task_record.serialize()?,
        )
        .await?;

    // TODO: Also

    // TOOD: Now bring up a local manager instance and use it to start a manager job
    // on the cluster.

    // Done!

    Ok(())
}

async fn run_list(cmd: ListCommand) -> Result<()> {
    let meta_client = datastore::meta::client::MetastoreClient::create().await?;

    let nodes = meta_client.list("/").await?;
    for kv in nodes {
        println!("{:?}", kv);
    }

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

    for volume in task_spec.volumes_mut() {
        if let container::TaskSpec_VolumeSourceCase::BuildTarget(target) = volume.source_case() {
            println!("Building volume target: {}", target);

            let res = builder::run_build(target).await?;
            if res.output_files.len() != 1 {
                return Err(err_msg(
                    "Expected exactly one output for volume build target",
                ));
            }

            let data = fs::read(&res.output_files[0]).await?;

            let hash = {
                let mut hasher = SHA256Hasher::default();
                let hash = hasher.finish_with(&data);
                format!("sha256:{}", common::hex::encode(hash))
            };

            println!("=> Upload Blob: {}", hash);
            volume.set_blob_id(&hash);

            if existing_blobs.contains(&hash) {
                println!("Already uploaded");
                continue;
            }

            let mut blob_data = container::BlobData::default();
            blob_data.spec_mut().set_id(hash);
            blob_data.spec_mut().set_size(data.len() as u64);
            // TODO: Get this info from the build system.
            blob_data
                .spec_mut()
                .set_format(container::BlobFormat::TAR_GZ_ARCHIVE);
            blob_data.set_data(data);

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
    }

    // TODO: Interactive exec style runs should be interactive in the sense that
    // when the client's connection is closed, the container should also be
    // killed.

    println!("Starting server");

    let mut start_request = container::StartTaskRequest::default();
    start_request.set_task_spec(task_spec.clone());

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
    }
}

fn main() -> Result<()> {
    task::block_on(run())
}
