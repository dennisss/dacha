use std::{convert::TryFrom, sync::Arc};

use cluster_client::{
    meta::{client::ClusterMetaClient, NodeMetadataTable, WorkerMetadataTable},
    service::address::{ServiceEntity, ServiceName},
};
use common::errors::*;
use container_proto::cluster::{BundleBlobStoreStub, ContainerNodeStub, ManagerStub};
use db_table::query_one;

pub struct NodeStubs {
    pub service: ContainerNodeStub,
    pub blobs: BundleBlobStoreStub,
}

/// TODO: Support nodes in other zones.
pub async fn connect_to_node_id(
    meta_client: Arc<ClusterMetaClient>,
    node_id: u64,
) -> Result<NodeStubs> {
    let addr = ServiceName::for_node(meta_client.zone(), node_id)?.to_string();

    let channel = cluster_client::service::create_rpc_channel(&addr, meta_client).await?;

    Ok(NodeStubs {
        service: ContainerNodeStub::new(channel.clone()),
        blobs: BundleBlobStoreStub::new(channel.clone()),
    })
}

pub async fn connect_to_manager(meta_client: Arc<ClusterMetaClient>) -> Result<ManagerStub> {
    let manager_channel = cluster_client::service::create_rpc_channel(
        "manager.system.job.local.cluster.internal",
        meta_client,
    )
    .await?;

    let manager_stub = ManagerStub::new(manager_channel);

    Ok(manager_stub)
}

/// Helper for connecting to the node on which a specific worker exists.
#[derive(Args)]
pub struct WorkerNodeSelector {
    /// Name of the worker from which to
    pub worker_name: String,
}

impl WorkerNodeSelector {
    pub async fn connect(
        &self, meta_client: Arc<ClusterMetaClient>
    ) -> Result<NodeStubs> {
        let db = meta_client.db();
        let worker_meta =
            query_one!(db, WorkerMetadataTable, "spec.name = ?", &self.worker_name)
                .ok_or_else(|| format_err!("No worker named: {}", self.worker_name))?;

        connect_to_node_id(meta_client, worker_meta.assigned_node()).await
    }
}

