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
/// This is set by the Node runtime.
pub const worker_name_ENV_VAR: &'static str = "CLUSTER_WORKER";
