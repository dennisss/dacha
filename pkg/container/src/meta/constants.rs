use std::time::Duration;

/// Time in between attempts by the node to refresh it's 'last_seen' time in the
/// metastore.
pub const NODE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// If a node's 'last_seen' hasn't changed in this amount of time, we will
/// consider it to be dead.
pub const NODE_TIMEOUT: Duration = Duration::from_secs(120);

/// Environment variable containing the name of the zone in which a Worker is
/// currently running.
///
/// This is used by the ClusterMetaClient to connect to the correct servers.
///
/// This is set by the Node runtime.
pub const ZONE_ENV_VAR: &'static str = "CLUSTER_ZONE";

/// Environment variable containing the id of the node running the Worker.
///
/// This is set by the Node runtime.
pub const NODE_ID_ENV_VAR: &'static str = "CLUSTER_NODE";

/// Environment variable containing the name of the currently running Worker.
///
/// TODO: This should be used by the server to verify the host name provided.
/// (Host name must also align with the TLS name)
///
/// This is set by the Node runtime.
pub const WORKER_NAME_ENV_VAR: &'static str = "CLUSTER_WORKER";

/// Environment variable containing a URI for connecting to a metastore.
///
/// This will be set by the Node runtime to point to either the metastore itself
/// or a proxy. This should always contain an ip address host as it can't detect
/// on the meta store for resolving the address.
pub const META_STORE_ADDR_ENV_VAR: &'static str = "CLUSTER_META_STORE";
