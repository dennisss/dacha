use std::convert::TryInto;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use common::async_std::sync::Mutex;
use common::errors::*;
use common::task::ChildTask;
use datastore::meta::client::MetastoreClient;
use http::ResolvedEndpoint;

use crate::meta::client::ClusterMetaClient;
use crate::meta::GetClusterMetaTable;
use crate::proto::meta::*;
use crate::service::address::*;

/// Resolves the addresses of cluster services to useable ip/port numbers.
///
/// We assume that all host names that end in '.cluster.internal' are in the
/// cluster.
///
/// We accept the following formats of addresses:
///                         "[node_id].node.[zone].cluster.internal"
/// "_[port_name].[worker_id].[job_name].worker.[zone].cluster.internal"
/// "_[port_name].          [job_name] .job.[zone].cluster.internal"
///
/// TODO: Consider restricting job_names to only be a 2 dot delimited labels so
/// that we can specify a job/worker address without a port.
///
/// With the following definitions for the above parameters:
/// - "[zone]" : Name of the cluster from which to look up objects or a special
///   value of "local" to retrieve from the current cluster.
/// - "[node_id]" : Id of the node to access or a special value of "self"
/// - "_[port_name[": Name of the port which should be requested. This is
///   optional and if not present we will use the first port defined for the
///   job/worker.
///
/// TODO: Verify that job_name doesn't start with '_'.
///
/// NOTE: Currently only a zone of "local" or equivalent is supported.
///
/// NOTE: The host names have name segments reversed so to access worker 2 of
/// job "adder.server", the address will be
/// "_[port].2.server.adder.worker.[zone].cluster.internal"
///
/// TODO: Consider changing this to avoid name labels which consist only of
/// numbers.
pub struct ServiceResolver {
    shared: Arc<Shared>,
    background_task: ChildTask,
}

struct Shared {
    meta_client: Arc<ClusterMetaClient>,
    service_address: ServiceAddress,
    state: Mutex<State>,
}

struct State {
    resolved: Vec<http::ResolvedEndpoint>,
    listeners: Vec<http::ResolverChangeListener>,
}

impl ServiceResolver {
    /// Creates a service resolver which will fallback to using a regular system
    /// DNS based resolver if the address is not a cluster managed address.
    pub async fn create_with_fallback<F: Future<Output = Result<Arc<ClusterMetaClient>>>>(
        address: &str,
        meta_client_factory: F,
    ) -> Result<Arc<dyn http::Resolver>> {
        let authority: http::uri::Authority = address.try_into()?;

        if let http::uri::Host::Name(name) = &authority.host {
            if ServiceAddress::is_service_address(name) {
                return Ok(Arc::new(
                    // NOTE: We pass in the original 'address' so that it rejects addresses with
                    // ports specified.
                    Self::create(address, meta_client_factory.await?).await?,
                ));
            }
        }

        let port = authority
            .port
            .ok_or_else(|| err_msg("Address does not contain a port"))?;

        Ok(Arc::new(http::SystemDNSResolver::new(authority.host, port)))
    }

    /// TODO: Support having a fallback to a regular public DNS name if this
    /// resolver doesn't support it.
    pub async fn create(address: &str, meta_client: Arc<ClusterMetaClient>) -> Result<Self> {
        let zone = meta_client
            .cluster_table::<ZoneMetadata>()
            .get(&())
            .await?
            .ok_or_else(|| err_msg("No local zone defined"))?;

        let service_address = ServiceAddress::parse(address, zone.name())?;

        if service_address.name.zone != zone.name() {
            return Err(err_msg("Unsupported zone"));
        }

        let shared = Arc::new(Shared {
            meta_client,
            service_address,
            state: Mutex::new(State {
                resolved: vec![],
                listeners: vec![],
            }),
        });

        let background_task = ChildTask::spawn(Self::background_thread(shared.clone()));

        Ok(Self {
            shared,
            background_task,
        })
    }

    async fn background_thread(shared: Arc<Shared>) {
        // TODO: Implement using key watchers.

        loop {
            if let Err(e) = Self::background_thread_impl(shared.clone()).await {
                eprintln!("ServiceResolver failed: {}", e);
            }

            common::async_std::task::sleep(Duration::from_secs(10)).await;
        }
    }

    async fn background_thread_impl(shared: Arc<Shared>) -> Result<()> {
        // TODO: Ignore timed out nodes
        // TODO: Ignore non-healthy workers.

        let mut endpoints = vec![];

        match &shared.service_address.name.entity {
            ServiceEntity::Node { id } => {
                let address = Self::get_node_addr(&shared, *id).await?;
                endpoints.push(http::ResolvedEndpoint {
                    address,
                    authority: http::uri::Authority {
                        user: None,
                        host: http::uri::Host::Name(shared.service_address.name.to_string()),
                        port: None,
                    },
                });
            }
            ServiceEntity::Job { job_name } => {
                let workers = shared
                    .meta_client
                    .cluster_table::<WorkerMetadata>()
                    .list_by_job(job_name)
                    .await?;

                for worker in workers {
                    if let Some(endpoint) = Self::get_worker_endpoint(&shared, &worker).await? {
                        endpoints.push(endpoint);
                    }
                }
            }
            ServiceEntity::Worker {
                job_name,
                worker_id: worker_id,
            } => {
                let worker = shared
                    .meta_client
                    .cluster_table::<WorkerMetadata>()
                    .get(&format!("{}.{}", job_name, worker_id))
                    .await?
                    .ok_or_else(|| err_msg("Failed to find worker"))?;

                if let Some(endpoint) = Self::get_worker_endpoint(&shared, &worker).await? {
                    endpoints.push(endpoint);
                }
            }
        }

        {
            let mut state = shared.state.lock().await;

            state.resolved = endpoints;

            let mut i = 0;
            while i < state.listeners.len() {
                if !(state.listeners[i])() {
                    let _ = state.listeners.swap_remove(i);
                    continue;
                }

                i += 1;
            }
        }

        Ok(())
    }

    async fn get_worker_endpoint(
        shared: &Shared,
        worker: &WorkerMetadata,
    ) -> Result<Option<http::ResolvedEndpoint>> {
        let node_address = Self::get_node_addr(shared, worker.assigned_node()).await?;

        let mut port = None;
        for port_spec in worker.spec().ports() {
            if let Some(port_name) = &shared.service_address.port {
                if port_name != port_spec.name() {
                    continue;
                }
            }

            port = Some(port_spec.number());
        }

        // TODO: Log an error in this case?
        let port = match port {
            Some(v) => v,
            None => {
                return Ok(None);
            }
        };

        let address = SocketAddr::new(node_address.ip(), port as u16);

        let host_name =
            ServiceName::for_worker(&shared.service_address.name.zone, worker.spec().name())?
                .to_string();

        Ok(Some(ResolvedEndpoint {
            address,
            authority: http::uri::Authority {
                user: None,
                host: http::uri::Host::Name(host_name),
                port: None,
            },
        }))
    }

    async fn get_node_addr(shared: &Shared, id: u64) -> Result<SocketAddr> {
        let node_meta = shared
            .meta_client
            .cluster_table::<NodeMetadata>()
            .get(&id)
            .await?
            .ok_or_else(|| err_msg("Missing node"))?;

        let authority = node_meta.address().parse::<http::uri::Authority>()?;
        let ip: std::net::IpAddr = match &authority.host {
            http::uri::Host::IP(ip) => ip.clone().try_into()?,
            _ => {
                return Err(err_msg("NodeMetadata doesn't contain an ip address"));
            }
        };

        let port = authority.port.ok_or_else(|| err_msg("No port in route"))?;

        Ok(SocketAddr::new(ip, port))
    }
}

#[async_trait]
impl http::Resolver for ServiceResolver {
    async fn resolve(&self) -> Result<Vec<http::ResolvedEndpoint>> {
        // TODO: This should probably error out in some cases so that we can leverage
        // the LoadBalancedClient backoff logic to help retry communicating with cluster
        // metadata.

        Ok(self.shared.state.lock().await.resolved.clone())
    }

    async fn add_change_listener(&self, listener: http::ResolverChangeListener) {
        let mut state = self.shared.state.lock().await;
        state.listeners.push(listener);
    }
}
