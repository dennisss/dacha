use std::convert::TryInto;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use common::async_std::sync::Mutex;
use common::errors::*;
use common::task::ChildTask;
use datastore::meta::client::MetastoreClient;
use http::ResolvedEndpoint;

use crate::meta::GetClusterMetaTable;
use crate::proto::meta::*;
use crate::service::address::*;

/// Resolves the addresses of cluster services to useable ip/port numbers.
///
/// We assume that all host names that end in '.cluster.internal' are in the
/// cluster.
///
/// We accept the following formats of addresses:
///               "[node_id].node.[zone].cluster.internal"
/// "[task_index].[job_name].task.[zone].cluster.internal:[port_name]"
///              "[job_name] .job.[zone].cluster.internal:[port_name]"
///
/// With the following definitions for the above parameters:
/// - "[zone]" : Name of the cluster from which to look up objects or a special
///   value of "local" to retrieve from the current cluster.
/// - "[node_id]" : Id of the node to access or a special value of "self"
///
/// NOTE: Currently only a zone of "local" or equivalent is supported.
///
/// NOTE: The host names have name segments reversed so to access task 2 of job
/// "adder.server", the address will be
/// "2.server.adder.task.[zone].cluster.internal"
///
/// TODO: Consider changing this to avoid name labels which consist only of
/// numbers.
pub struct ServiceResolver {
    shared: Arc<Shared>,
    background_task: ChildTask,
}

struct Shared {
    meta_client: Arc<MetastoreClient>,
    service_address: ServiceAddress,
    state: Mutex<State>,
}

struct State {
    resolved: Vec<http::ResolvedEndpoint>,
    listeners: Vec<http::ResolverChangeListener>,
}

impl ServiceResolver {
    /// TODO: Support having a fallback to a regular public DNS name if this
    /// resolver doesn't support it.
    pub async fn create(address: &str, meta_client: Arc<MetastoreClient>) -> Result<Self> {
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
        // TODO: Ignore non-healthy tasks.

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
                let tasks = shared
                    .meta_client
                    .cluster_table::<TaskMetadata>()
                    .get_prefix(&format!("{}.", job_name))
                    .await?;

                for task in tasks {
                    if let Some(endpoint) =
                        Self::get_task_endpoint(&shared, job_name.as_str(), &task).await?
                    {
                        endpoints.push(endpoint);
                    }
                }
            }
            ServiceEntity::Task {
                job_name,
                task_index,
            } => {
                let task = shared
                    .meta_client
                    .cluster_table::<TaskMetadata>()
                    .get(&format!("{}.{}", job_name, task_index))
                    .await?
                    .ok_or_else(|| err_msg("Failed to find task"))?;

                if let Some(endpoint) =
                    Self::get_task_endpoint(&shared, job_name.as_str(), &task).await?
                {
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

    async fn get_task_endpoint(
        shared: &Shared,
        job_name: &str,
        task: &TaskMetadata,
    ) -> Result<Option<http::ResolvedEndpoint>> {
        let task_index = {
            task.spec()
                .name()
                .split('.')
                .last()
                .ok_or_else(|| err_msg("Missing last label in task name"))?
                .parse::<usize>()?
        };

        let node_address = Self::get_node_addr(shared, task.assigned_node()).await?;

        let mut port = None;
        for port_spec in task.spec().ports() {
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

        let host_name = ServiceName {
            zone: shared.service_address.name.zone.clone(),
            entity: ServiceEntity::Task {
                job_name: job_name.to_string(),
                task_index,
            },
        }
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
