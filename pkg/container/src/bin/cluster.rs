// TODO: Combine all of the CLI utilities into this one.

#[macro_use]
extern crate common;
#[macro_use]
extern crate macros;

use std::sync::Arc;

use common::async_std::fs;
use common::async_std::task;
use common::errors::*;
use container::ContainerNodeStub;
use container::JobMetadata;
use container::{TaskMetadata, TaskSpec, ZoneMetadata};
use protobuf::text::ParseTextProto;
use protobuf::Message;

#[derive(Args)]
pub struct Args {
    command: Command,
}

#[derive(Args)]
enum Command {
    #[arg(name = "bootstrap")]
    Bootstrap(BootstrapCommand),

    #[arg(name = "list")]
    List(ListCommand),
}

#[derive(Args)]
struct BootstrapCommand {
    node: String,
}

#[derive(Args)]
struct ListCommand {}

async fn new_stub(node_addr: &str) -> Result<ContainerNodeStub> {
    let channel = Arc::new(rpc::Http2Channel::create(http::ClientOptions::from_uri(
        &format!("http://{}", node_addr).parse()?,
    )?)?);

    let stub = container::ContainerNodeStub::new(channel);

    Ok(stub)
}

/// Assuming that we have a single
async fn run_bootstrap(cmd: BootstrapCommand) -> Result<()> {
    let node_stub = new_stub(&cmd.node).await?;
    let node_meta = node_stub
        .Identity(
            &rpc::ClientRequestContext::default(),
            &google::proto::empty::Empty::default(),
        )
        .await
        .result?;
    let node_id = node_meta.id();

    println!("Bootstrapping cluster with node {:08x}", node_id);

    // TODO: Need to build any build volumes to blobs.
    let meta_task_spec = TaskSpec::parse_text(
        &fs::read_to_string(project_path!("pkg/datastore/config/metastore.task")).await?,
    )?;

    // TODO: Assign a port to the spec.

    // TODO: Build the step and add it to the node

    // TODO: Call bootstrap on the metastore instance (main challenge is to ensure
    // that only one metastore group exists if we need to retry bootstrapping).

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

async fn run() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;
    match args.command {
        Command::Bootstrap(cmd) => run_bootstrap(cmd).await,
        Command::List(cmd) => run_list(cmd).await,
    }
}

fn main() -> Result<()> {
    task::block_on(run())
}
