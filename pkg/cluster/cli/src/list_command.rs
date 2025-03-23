use std::collections::HashMap;

use cluster_client::meta::client::ClusterMetaClient;
use cluster_client::meta::table::*;
use common::errors::*;
use container_proto::cluster::*;

use crate::utils::{connect_to_node, connect_to_node_id, NodeStubs};

#[derive(Args)]
pub struct ListCommand {
    /// What type of objects to enumerate. If not specified, we will enumerate
    /// all objects.
    #[arg(positional)]
    kind: Option<ObjectKind>,

    /// Address of the node from which to query the objects.
    ///
    /// NOTE: Note all object kinds will be supported in this mode.
    node_addr: Option<String>,

    node_id: Option<u64>,
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

pub async fn run_list(cmd: ListCommand) -> Result<()> {
    let creds = cluster_client::credentials::get_cluster_credentials().await?;

    if let Some(node_addr) = &cmd.node_addr {
        let node = connect_to_node(node_addr, Some(creds.client_options())).await?;
        run_list_on_node(node).await?;
        return Ok(());
    }

    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let db = meta_client.db();

    if let Some(node_id) = &cmd.node_id {
        let node = connect_to_node_id(meta_client.clone(), *node_id).await?;
        run_list_on_node(node).await?;
        return Ok(());
    }

    let kind = cmd.kind.unwrap();

    match kind {
        ObjectKind::Node => {
            println!("Nodes:");
            let nodes = db.list::<NodeMetadataTable>().await?;
            for node in nodes {
                println!("{:?}", node);
            }
        }
        ObjectKind::Job => {
            println!("Jobs:");
            let jobs = db.list::<JobMetadataTable>().await?;
            for job in jobs {
                println!("{:?}", job);
            }
        }
        ObjectKind::Worker => {
            let mut node_workers = HashMap::new();
            {
                let request_context = rpc::ClientRequestContext::default();
                let nodes = db.list::<NodeMetadataTable>().await?;
                for node in nodes {
                    let node_stubs = connect_to_node_id(meta_client.clone(), node.id()).await?;
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
            let workers = db.list::<WorkerMetadataTable>().await?;

            let worker_states = db
                .list::<WorkerStateMetadataTable>()
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
            let nodes = db.list::<BundleBlobMetadataTable>().await?;
            for node in nodes {
                println!("{:?}", node);
            }
        }
    }

    Ok(())
}

async fn run_list_on_node(node: NodeStubs) -> Result<()> {
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

    Ok(())
}
