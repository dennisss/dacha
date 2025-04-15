// General environment variables that are populated when running in a cluster.

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
///
/// TODO: Move this and most of the other ones out of the 'meta' sub-directory
pub const WORKER_NAME_ENV_VAR: &'static str = "CLUSTER_WORKER";

/// Environment variable containing the path to the directory containing
/// client/server TLS certificates and keys to use.
///
/// AVOID READING THIS DIRECTLY. Prefer to use
/// ClusterMetaClient::create_from_environment.
pub const CREDENTIALS_DIR_ENV_VAR: &'static str = "CLUSTER_CREDENTIALS";
