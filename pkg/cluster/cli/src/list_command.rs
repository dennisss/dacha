use std::collections::HashMap;
use std::sync::Arc;

use cluster_client::ClusterMetaClient;
use cluster_client::meta::table::*;
use common::errors::*;
use container_proto::cluster::*;
use cluster_client::id::{entity_id_to_string, entity_id_from_string};
use cluster_client::service::address::ServiceName;
use base_units::ByteCount;
use terminal::TerminalTableBuilder;
use db_table::query;

use crate::utils::{connect_to_node_id, NodeStubs};

#[derive(Args)]
pub struct ListCommand {
    /// What type of objects to enumerate. If not specified, we will enumerate
    /// all objects.
    #[arg(positional)]
    kind: Option<ObjectKind>,

    /// NOTE: Note all object kinds will be supported in this mode.
    node_id: Option<String>,
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
    let meta_client = ClusterMetaClient::create_from_environment().await?;
    let db = meta_client.db();

    if let Some(node_id) = &cmd.node_id {
        let node_id = entity_id_from_string(node_id.as_str()).ok_or_else(|| err_msg("Invalid node_id"))?;

        let node = connect_to_node_id(meta_client.clone(), node_id).await?;
        run_list_on_node(node).await?;
        return Ok(());
    }

    let kind = cmd.kind.unwrap();

    match kind {
        ObjectKind::Node => {
            let nodes = db.list::<NodeMetadataTable>().await?;
            let node_scheduling = db.list::<NodeSchedulingMetadataTable>().await?;

            let mut node_scheduling_by_id = HashMap::new();
            for v in node_scheduling {
                node_scheduling_by_id.insert(v.node_id(), v);
            }

            let mut table = TerminalTableBuilder::new();
            table.row().col("ID").col("ADDRESS").col("LABELS");

            for node in nodes {

                let mut labels = String::new();

                if let Some(meta) = node_scheduling_by_id.get(&node.id()) {
                    for l in meta.labels().label() {
                        if !labels.is_empty() {
                            labels.push(',');
                        }

                        labels.push_str(&format!("{}={}", l.key(), l.value()));
                    }
                }

                table.row().col(entity_id_to_string(node.id()).unwrap()).col(node.address()).col(labels);
            }

            table.print();
        }
        ObjectKind::Job => {
            let mut table = TerminalTableBuilder::new();
            table.row().col("NAME").col("REPLICAS");

            let jobs = db.list::<JobMetadataTable>().await?;
            
            // let workers = db.list::<WorkerMetadataTable>().await?;

            for job in jobs {

                let mut name = job.spec().name().to_string();
                if job.spec().worker().ports().len() > 0 {
                    let url = format!("https://{}", ServiceName::for_job(meta_client.zone(), &name)?.to_string());
                    name = format!("{}{}{}", terminal::start_hyperlink(&url), name, terminal::start_hyperlink(""));
                }

                table.row().col(name).col(job.spec().replicas().to_string());
            }

            table.print();
        }
        ObjectKind::Worker => {
            let mut node_workers = HashMap::new();
            let mut node_map = HashMap::new();
            {
                let nodes = db.list::<NodeMetadataTable>().await?;
                for node in nodes {
                    if let Err(e) = get_workers_from_node(meta_client.clone(), node.id(), &mut node_workers).await {
                        eprintln!("Failed to contact node {}: {}", entity_id_to_string(node.id()).unwrap(), e);
                    }
                    node_map.insert(node.id(), node);
                }
            }

            let workers = db.list::<WorkerMetadataTable>().await?;

            let worker_states = db
                .list::<WorkerStateMetadataTable>()
                .await?
                .into_iter()
                .map(|s| (s.worker_name().to_string(), s))
                .collect::<HashMap<_, _>>();

            let mut table = TerminalTableBuilder::new();

            table.row().col("NAME").col("NODE ID").col("STATE").col("NODE STATE").col("NODE_IP:PORT");

            for worker in workers {
                let worker_state = worker_states
                    .get(worker.spec().name())
                    .cloned()
                    .unwrap_or_default();

                let mut node_state = String::new();
                if let Some(node_worker) = node_workers.get(worker.spec().name()) {
                    node_state = format!("({:?})", node_worker.state());
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

                let mut name = worker.spec().name().to_string();
                if worker.spec().ports().len() > 0 {

                    let url = format!("https://{}", ServiceName::for_worker(meta_client.zone(), &name)?.to_string());

                    name = format!("{}{}{}", terminal::start_hyperlink(&url), name, terminal::start_hyperlink(""));
                }

                let mut addrs = vec![];
                for port in worker.spec().ports() {
                    let node =  node_map.get(&worker.assigned_node()).unwrap();
                    let authority = node.address().parse::<http::uri::Authority>()?;
                    let ip = match &authority.host {
                        http::uri::Host::IP(ip) => ip.clone(),
                        _ => {
                            return Err(err_msg("NodeMetadata doesn't contain an ip address"));
                        }
                    };

                    addrs.push(format!("{}:{}", ip.to_string(), port.number()));
                }

                table.row()
                .col(name)
                .col(entity_id_to_string(worker.assigned_node()).unwrap())
                .col(format!("{:?}", state))
                .col(node_state)
                .col(addrs.join(", "));
            }

            table.print();
        }
        ObjectKind::Blob => {
            let blobs = db.list::<BundleBlobMetadataTable>().await?;

            let mut table = TerminalTableBuilder::new();
            table.row().col("ID").col("SIZE").col("REPLICAS");

            for blob in blobs {
                let blob_replicas = query!(db, BundleBlobReplicaTable, "blob_id = ?", blob.spec().id());

                let mut repls = blob_replicas
                    .iter()
                    .map(|r| {
                        
                        let id = entity_id_to_string(r.node_id()).unwrap();
                        
                        if r.uploaded() {
                            id
                        } else {
                            format!("[{}]", id)
                        }
                    })
                    .collect::<Vec<String>>()
                    .join(",");

                table.row()
                .col(blob.spec().id())
                .col(format!("{:?}", ByteCount::from(blob.spec().size() as usize)))
                .col(repls);
            }

            table.print();
        }
    }

    Ok(())
}

async fn get_workers_from_node(
    meta_client: Arc<ClusterMetaClient>,
    node_id: u64,
    node_workers: &mut HashMap<String, WorkerProto>
) -> Result<()> {
    let request_context = rpc::ClientRequestContext::default();
    let node_stubs = connect_to_node_id(meta_client, node_id).await?;
    let res = node_stubs
        .service
        .ListWorkers(&request_context, &ListWorkersRequest::default())
        .await
        .result?;

    for worker in res.workers() {
        node_workers.insert(worker.spec().name().to_string(), worker.as_ref().clone());
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
