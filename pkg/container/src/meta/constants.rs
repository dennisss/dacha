use std::time::Duration;

/// Time in between attempts by the node to refresh it's 'last_seen' time in the
/// metastore.
pub const NODE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// If a node's 'last_seen' hasn't changed in this amount of time, we will
/// consider it to be dead.
pub const NODE_TIMEOUT: Duration = Duration::from_secs(120);
