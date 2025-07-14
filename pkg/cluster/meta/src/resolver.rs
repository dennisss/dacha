use std::sync::Arc;

use common::errors::*;
use container_proto::cluster::*;
use db_table::db::{ProtobufDB, ProtobufDBTransaction};
use db_table::{query, query_one};
use net::ip::SocketAddr;
use cluster_client::meta::{NodeMetadataTable, WorkerMetadataTable};
use cluster_client::service::address::*;
use cluster_client::ClusterServerConnectionData;
use cluster_client::acl::checker::check_entity_allowed;
use cluster_client::acl::principal::{Principal, PrincipalSet};

/// Service running in the cluster metastore for resolving service addresses.
///
/// Users would interface with this via the cluster_client::ServiceResolver
/// struct.
pub struct ServiceResolverImpl {
    db: Arc<ProtobufDB>,
    zone: String,
}

impl ServiceResolverImpl {

    pub fn new(db: Arc<ProtobufDB>, zone: &str) -> Self {
        Self { db, zone: zone.to_string() }
    }

    async fn resolve_impl(
        &self,
        address: &str,
        context: &rpc::ServerRequestContext,
    ) -> Result<ServiceResolverResponse> {
        let service_address = ServiceAddress::parse_relative_addr(address, &self.zone)
            .map_err(|_| rpc::Status::invalid_argument("Invalid service address"))?;

        if service_address.name.zone() != self.zone {
            return Err(rpc::Status::invalid_argument("Unsupported zone").into());
        }

        if !service_address.name.maybe_reachable() {
            return Err(rpc::Status::invalid_argument(
                format!("Can not connect to {}", address)
            ).into());
        }

        if !self.check_allowed_to_resolve(&service_address, context).await? {
            return Err(rpc::Status::permission_denied("Not allowed to resolve this address").into());
        }

        let mut out = ServiceResolverResponse::default();
        let txn = self.db.new_transaction().await?;
        Self::get_endpoints(&service_address, &txn, &mut out).await?;
        Ok(out)
    }

    async fn check_allowed_to_resolve(
        &self,
        service_address: &ServiceAddress,
        context: &rpc::ServerRequestContext
    ) -> Result<bool> {
        let conn = ClusterServerConnectionData::from_rpc_context(context)?;

        let mut allowed_principals = PrincipalSet::default();

        // TODO: Eventually move the ACLs to the JobMetadata?
        let mut allow_unauthenticated = false;
        match &service_address.name.entity() {
            ServiceEntity::Job { job_name } => {
                // Minimal set of jobs needed for user login.
                if job_name == "system.meta" || job_name == "system.cert-authority" {
                    allow_unauthenticated = true;
                }
            }
            _ => {}
        }

        if allow_unauthenticated {
            allowed_principals.insert(Principal::Unauthenticated);
        } else {
            allowed_principals.insert(Principal::Group {
                zone: self.zone.clone(),
                name: "cluster-clients".to_string()
            });
        }

        check_entity_allowed(
            conn.peer.as_ref(),
            &allowed_principals,
            &self.zone,
            Some(self.db.as_ref()),
        ).await
    }


    async fn get_endpoints(
        service_address: &ServiceAddress,
        txn: &ProtobufDBTransaction<'_>,
        out: &mut ServiceResolverResponse
    ) -> Result<()> {
        // TODO: Ignore timed out nodes
        // TODO: Ignore non-healthy workers.

        match &service_address.name.entity() {
            ServiceEntity::Node { id } => {
                if let Some(address) = Self::get_node_addr(*id, txn).await? {
                    let mut ep = out.new_endpoints();
                    ep.set_name("");
                    ep.set_address(address.to_string());
                    ep.set_hostname(service_address.name.to_string());
                }
            }
            ServiceEntity::Job { job_name } => {
                let workers = query!(
                    txn,
                    WorkerMetadataTable,
                    "STARTS_WITH(spec.name, ?)",
                    format!("{}.", job_name)
                );

                for worker in workers {
                    if let Some(endpoint) = Self::get_worker_endpoint(&service_address, &worker, txn).await? {
                        out.add_endpoints(endpoint);
                    }
                }
            }
            ServiceEntity::Worker {
                job_name,
                worker_id,
            } => {
                let worker = query_one!(
                    txn,
                    WorkerMetadataTable,
                    "spec.name = ?",
                    format!("{}.{}", job_name, worker_id)
                )
                .ok_or_else(|| err_msg("Failed to find worker"))?;

                // TODO: Must check worker state metadata.

                if let Some(endpoint) = Self::get_worker_endpoint(&service_address, &worker, txn).await? {
                    out.add_endpoints(endpoint);
                }
            }
            _ => {
                // This should be caught earlier by the 'maybe_reachable' check.
                return Err(format_err!("Can't connect to service"));
            }
        }

        Ok(())
    }

    async fn get_worker_endpoint(
        service_address: &ServiceAddress,
        worker: &WorkerMetadata,
        txn: &ProtobufDBTransaction<'_>
    ) -> Result<Option<ResolvedEndpointProto>> {
        // NOTE: Within a txn, this should be cacheable if there are multiple
        // workers on one node.
        let node_address = match Self::get_node_addr(worker.assigned_node(), txn).await? {
            Some(v) => v,
            None => return Ok(None)
        };

        // TOOD: Must restrict to only healthy workers (so we must look at
        // WorkerStateMetadata).

        let mut port = None;
        for port_spec in worker.spec().ports() {
            if let Some(port_name) = &service_address.port {
                if port_name != port_spec.name() {
                    continue;
                }
            }

            // TODO: Can I dynamically determine whether to use TLS here?

            port = Some(port_spec.number());
        }

        // TODO: Log an error in this case?
        let port = match port {
            Some(v) => v,
            None => {
                return Ok(None);
            }
        };

        let address = SocketAddr::new(node_address.ip().clone(), port as u16);

        let host_name =
            ServiceName::for_worker(&service_address.name.zone(), worker.spec().name())?
                .to_string();

        let mut ep = ResolvedEndpointProto::default();
        ep.set_name(worker.spec().name());
        ep.set_address(address.to_string());
        ep.set_hostname(host_name);
        Ok(Some(ep))
    }

    /// Retrieves and validates the ip:port address for a single node.
    ///
    /// NOTE: This is derived from the NodeMetadata table which nodes self report
    /// and thus may not be well trustable.
    ///
    /// Will return None if there is no valid address for the node.
    async fn get_node_addr(id: u64, txn: &ProtobufDBTransaction<'_>) -> Result<Option<SocketAddr>> {
        let node_meta = query_one!(txn, NodeMetadataTable, "id = ?", id);

        match Self::get_node_addr_inner(node_meta) {
            Ok(v) => Ok(Some(v)),
            Err(e) => {
                eprintln!("Failed to get node address for node id {}: {}", id, e);
                Ok(None)
            }
        }
    }

    fn get_node_addr_inner(node_meta: Option<NodeMetadata>) -> Result<SocketAddr> {
        let node_meta = node_meta.ok_or_else(|| err_msg("Missing node metadata"))?;

        let authority = node_meta.address().parse::<http::uri::Authority>()?;
        let ip = match &authority.host {
            http::uri::Host::IP(ip) => ip.clone(),
            _ => {
                return Err(err_msg("NodeMetadata doesn't contain an ip address"));
            }
        };

        let port = authority.port.ok_or_else(|| err_msg("No port in route"))?;

        Ok(SocketAddr::new(ip, port))
    }

}

#[async_trait]
impl ServiceResolverService for ServiceResolverImpl {
    async fn Resolve(
        &self,
        request: rpc::ServerRequest<ServiceResolverRequest>,
        response: &mut rpc::ServerResponse<ServiceResolverResponse>,
    ) -> Result<()> {
        response.value = self.resolve_impl(request.value.address(), &request.context).await?;
        Ok(())
    }
}
