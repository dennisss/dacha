use std::time::Duration;

/// Time in between attempts by the node to refresh it's 'last_seen' time in the
/// metastore.
pub const NODE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// If a node's 'last_seen' hasn't changed in this amount of time, we will
/// consider it to be dead.
pub const NODE_TIMEOUT: Duration = Duration::from_secs(120);

/// Environment variable containing a URI for connecting to a metastore.
///
/// This will be set by the Node runtime to point to either the metastore itself
/// or a proxy. This should always contain an ip address host as it can't detect
/// on the meta store for resolving the address.
pub const META_STORE_ADDR_ENV_VAR: &'static str = "CLUSTER_META_STORE";

/// Environment variable containing a comma separated list of seed server
/// addresses that can be used to find the cluster's Metastore instance.
///
/// TODO: revise this.
///
/// This is set by the Node runtime and used internally by the
/// ClusterMetaClient.
pub const META_STORE_SEEDS_ENV_VAR: &'static str = "CLUSTER_META_SEEDS";
