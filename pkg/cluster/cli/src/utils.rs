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

// TODO: Improve or get rid of this since we can't determine the hostname of a
// node before connecting to it for TLS.
pub async fn connect_to_node(
    node_addr: &str,
    tls_options: Option<crypto::tls::ClientOptionsContainer>,
) -> Result<NodeStubs> {
    let mut options = rpc::Http2ChannelOptions::try_from(format!("http://{}", node_addr).as_str())?;
    options.http.backend_balancer.backend.tls = tls_options;
    options.base_path = "/rpc".into();

    let channel = Arc::new(rpc::Http2Channel::create(options).await?);

    Ok(NodeStubs {
        service: ContainerNodeStub::new(channel.clone()),
        blobs: BundleBlobStoreStub::new(channel.clone()),
    })
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

#[derive(Args)]
pub struct WorkerNodeSelector {
    /// Name of the worker from which to
    pub worker_name: String,

    pub node_selector: NodeSelector,
    /* TODO: Provide the attempt_id here as it may influence us to use a differnet node (one
     * that was previously assigned the worker)
     * - Given the attempt_id as a timestamp, we can search the WorkerMetadata in the metastore
     *   for the version of that record that was active at the time of the attempt (but need to
     *   be careful about checking ACLs for logs in this case) */
}

impl WorkerNodeSelector {
    pub async fn connect(
        &self,
        tls_options: Option<crypto::tls::ClientOptionsContainer>,
    ) -> Result<NodeStubs> {
        let node_addr = match self.node_selector.get_node_address().await? {
            Some(addr) => addr,
            None => {
                // Must connect to the metastore, find the worker, and then we can

                let meta_client = ClusterMetaClient::create_from_environment().await?;
                let db = meta_client.db();

                let worker_meta =
                    query_one!(db, WorkerMetadataTable, "spec.name = ?", &self.worker_name)
                        .ok_or_else(|| format_err!("No worker named: {}", self.worker_name))?;

                // TODO: assigned_node may eventually be allowed to be zero.
                let node_meta =
                    query_one!(db, NodeMetadataTable, "id = ?", worker_meta.assigned_node())
                        .ok_or_else(|| err_msg("Failed to find node for worker"))?;

                node_meta.address().to_string()
            }
        };

        connect_to_node(&node_addr, tls_options).await
    }
}

#[derive(Args)]
pub struct NodeSelector {
    node_addr: Option<String>,

    node_id: Option<u64>,
}

impl NodeSelector {
    async fn get_node_address(&self) -> Result<Option<String>> {
        if self.node_addr.is_some() && self.node_id.is_some() {
            return Err(err_msg("Ambigious node selector"));
        }

        if let Some(addr) = self.node_addr.clone() {
            return Ok(Some(addr));
        }

        if let Some(id) = self.node_id {
            let meta_client = ClusterMetaClient::create_from_environment().await?;
            let db = meta_client.db();

            let node_meta = query_one!(db, NodeMetadataTable, "id = ?", id)
                .ok_or_else(|| err_msg("Failed to find node for worker"))?;

            return Ok(Some(node_meta.address().to_string()));
        }

        Ok(None)
    }
}
