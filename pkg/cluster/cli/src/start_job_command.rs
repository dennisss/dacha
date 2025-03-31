use std::collections::HashMap;
use std::convert::TryFrom;
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime};
use std::{collections::HashSet, sync::Arc};

use builder::proto::{BundleBlobFormat, BundleSpec};
use cluster_client::credentials::get_cluster_credentials;
use cluster_client::env::ZONE_ENV_VAR;
use cluster_client::meta::client::ClusterMetaClient;
use cluster_client::meta::constants::META_STORE_SEEDS_ENV_VAR;
use cluster_client::meta::*;
use cluster_client::service::address::{ServiceAddress, ServiceName};
use cluster_client::service::create_rpc_channel;
use common::errors::*;
use common::failure::ResultExt;
use common::io::{Readable, Writeable};
use container::manager::Manager;
use container::{
    AllocateBundleBlobsRequest, AllocateBundleBlobsResponse, BundleBlobMetadata, JobSpec,
    ListWorkersRequest, ManagerIntoService, ManagerStub, NodeMetadata, StartJobRequest,
    WorkerStateMetadata_ReportedState,
};
use container::{
    ContainerNodeStub, Label, Labels, WorkerMetadata, WorkerSpec, WorkerSpec_Port,
    WorkerSpec_Volume, WorkerStateMetadata, WriteInputRequest,
};
use crypto::hasher::Hasher;
use crypto::sha256::SHA256Hasher;
use crypto::sip::SipHasher;
use db_table::query_one;
use executor::cancellation::AlreadyCancelledToken;
use executor::child_task::ChildTask;
use executor::JoinHandle;
use executor_multitask::ServiceResource;
use file::LocalPathBuf;
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

use crate::utils::*;

#[derive(Args)]
pub struct StartJobCommand {
    #[arg(positional)]
    job_spec_path: String,
}

#[derive(Args)]
pub struct StartWorkerCommand {
    #[arg(positional)]
    worker_spec_path: String,

    /// Should be of the 'ip:port'
    node_addr: String,
}

pub async fn run_start_worker(cmd: StartWorkerCommand) -> Result<()> {
    let creds = cluster_client::credentials::get_cluster_credentials().await?;

    let node = connect_to_node(&cmd.node_addr, Some(creds.client_options())).await?;

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

pub async fn run_start_job(cmd: StartJobCommand) -> Result<()> {
    let meta_client = ClusterMetaClient::create_from_environment().await?;

    let manager_stub = connect_to_manager(meta_client.clone()).await?;

    let job_spec = JobSpec::parse_text(&file::read_to_string(cmd.job_spec_path).await?)?;

    let request_context = rpc::ClientRequestContext::default();

    start_job_impl(meta_client, &manager_stub, &job_spec, &request_context).await
}

pub(crate) async fn start_job_impl(
    meta_client: Arc<ClusterMetaClient>,
    manager: &ManagerStub,
    job_spec: &JobSpec,
    request_context: &rpc::ClientRequestContext,
) -> Result<()> {
    let mut job_spec = job_spec.clone();
    let mut blobs = build_worker_blobs(job_spec.worker_mut()).await?;

    let blob_allocations = {
        let mut req = AllocateBundleBlobsRequest::default();
        for blob in &blobs {
            req.add_blob_specs(blob.spec().clone());
        }

        manager
            .AllocateBundleBlobs(request_context, &req)
            .await
            .result?
    };

    let blobs_by_id = blobs
        .into_iter()
        .map(|b| (b.spec().id().to_string(), b))
        .collect::<HashMap<_, _>>();

    // TODO: Should have a server side limit on how large individual request chunks
    // can be.

    // Upload blbos to all desired replicas.
    // TODO: Parallelize this
    for assignment in blob_allocations.new_assignments() {
        println!("Uploading: {:?}", assignment);

        let node = connect_to_node_id(meta_client.clone(), assignment.node_id()).await?;

        let blob_data = blobs_by_id
            .get(assignment.blob_id())
            .ok_or_else(|| err_msg("Missing blob"))?;

        upload_blob_to_node(&node, request_context, &blob_data).await?;
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

        upload_blob_to_node(node, request_context, &blob_data).await?;

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

async fn upload_blob_to_node(
    node: &NodeStubs,
    request_context: &rpc::ClientRequestContext,
    blob_data: &cluster_client::BlobData,
) -> Result<()> {
    let mut request = node.blobs.Upload(request_context).await;

    let start = Instant::now();

    // TODO: Need much better chunking here.
    request.send(blob_data).await;

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

    let end = Instant::now();

    println!("Uploaded in {:?}", end - start);

    Ok(())
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
